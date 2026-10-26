use super::*;
use crate::experiment::ArtifactPolicy;
use std::fs;

fn job() -> YosysJob {
    YosysJob {
        design: Design {
            files: vec![SourceFile::new("rtl/top.v", "module top; endmodule")],
            ..Design::default()
        },
        assets: Vec::new(),
        ..YosysJob::default()
    }
}

#[test]
fn compilation_stages_ordered_files_options_and_headers() {
    let mut job = job();
    job.design.files.push(SourceFile::new(
        "second file.sv",
        "module second; endmodule",
    ));
    job.design
        .headers
        .push(SourceFile::new("include dir/header.vh", "`define VALUE 4"));
    job.design.options.system_verilog = true;
    job.design.options.include_dirs.push("include dir".into());
    job.design
        .options
        .defines
        .insert("WIDTH".into(), Some("8".into()));
    let plan = job.compile().unwrap();
    assert!(
        plan.files()
            .iter()
            .any(|file| file.path == Path::new("include dir/header.vh"))
    );
    assert!(plan.script().contains("-sv"));
    assert!(plan.script().contains("-DWIDTH=8"));
    assert!(plan.script().contains("-I.yosys-includes/0"));
    assert!(plan.script().contains("\"rtl/top.v\" \"second file.sv\""));
    assert!(!plan.script().contains("header.vh"));
    assert!(!plan.script().contains("write_verilog"));
    assert_eq!(
        plan.includes(),
        &[(
            PathBuf::from(".yosys-includes/0"),
            PathBuf::from("../include dir")
        )]
    );
}

#[test]
fn compilation_rejects_escaping_and_conflicting_paths() {
    for path in [
        "../outside.v",
        "/outside.v",
        "run.ys",
        "rtl/top.v",
        "rtl/./top.v",
        ".yosys-logs/stat.log",
    ] {
        let mut job = job();
        job.assets.push(SourceFile::new(path, "conflict"));
        assert!(job.compile().is_err(), "{path}");
    }
}

#[test]
fn captured_logs_and_timings_decode_without_a_workspace() {
    let mut job = job();
    job.stage("opt", vec!["opt".into()]).unwrap();
    job.timings = true;
    let plan = job.compile().unwrap();
    let capture = || YosysCapture {
        process: ProcessOutput {
            output: crate::test_support::output(0, b"", b""),
            elapsed: Duration::from_millis(2),
            perf: None,
        },
        files: BTreeMap::from([
            (PathBuf::from("opt.log"), b"optimized".to_vec()),
            (
                PathBuf::from("timing.json"),
                br#"{"passes":{"opt":{"runtime_ns":1234}}}"#.to_vec(),
            ),
        ]),
    };
    let run = YosysRun::decode(&plan, capture()).unwrap();
    assert_eq!(run.log("opt").unwrap(), "optimized");
    assert_eq!(run.pass_time("opt").unwrap(), Duration::from_nanos(1234));
    for (name, contents) in [
        ("opt.log", None),
        ("timing.json", None),
        ("opt.log", Some(vec![255])),
        ("timing.json", Some(b"bad JSON".to_vec())),
    ] {
        let mut captured = capture();
        match contents {
            Some(bytes) => {
                captured.files.insert(name.into(), bytes);
            }
            None => {
                captured.files.remove(Path::new(name));
            }
        }
        assert!(YosysRun::decode(&plan, captured).is_err(), "{name}");
    }
}

#[test]
fn materialization_writes_the_compiled_files_and_cleans_up() {
    let root = tempfile::tempdir().unwrap();
    let mut job = job();
    job.design.options.include_dirs.push("empty include".into());
    let plan = job.compile().unwrap();
    let mut context = RunContext::new(root.path().into(), ArtifactPolicy::new(false, false, false));
    let prepared = YosysRunner {
        executable: "unused".into(),
    }
    .materialize(plan.clone(), &mut context)
    .unwrap();
    for file in plan.files() {
        assert_eq!(
            fs::read_to_string(prepared.directory.path().join(&file.path)).unwrap(),
            file.source
        );
    }
    assert!(prepared.directory.path().join("empty include").is_dir());
    assert!(prepared.directory.path().join(".yosys-includes/0").is_dir());
    drop(prepared);
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn statistics_count_leaf_cells_across_the_whole_hierarchy() {
    let text = r#"{"modules":{"\\top":{"num_cells":0,"num_cells_by_type":{"child":2}},"\\child":{"num_cells":1,"num_cells_by_type":{"NAND2_X1":1}}},"design":{"num_cells":2,"area":84.588,"num_cells_by_type":{"child":2,"NAND2_X1":2}}}"#;
    let stats = Statistics::parse(text).unwrap();
    assert_eq!(stats.cell_count, HashMap::from([("NAND2_X1".into(), 2)]));
    assert_eq!(stats.area, Some(84.588));
    assert_eq!(
        Statistics::parse(&text.replace("\"num_cells\":2,\"area\"", "\"num_cells\":4,\"area\""))
            .unwrap(),
        stats
    );
    for broken in [
        "",
        "{}",
        r#"{"modules":{}}"#,
        r#"{"modules":{"top":{"num_cells":2,"num_cells_by_type":{"$add":1}}}}"#,
        r#"{"modules":{"top":{"num_cells":0,"num_cells_by_type":{},"area":-1}}}"#,
    ] {
        assert!(Statistics::parse(broken).is_err());
    }
}

#[test]
fn runner_elaborates_the_design_and_keeps_execution_logs_fresh() {
    let root = tempfile::tempdir().unwrap();
    let mut context = RunContext::new(root.path().into(), ArtifactPolicy::default());
    let runner = YosysRunner::new(&Yosys::new(crate::test_support::tool(
        "yosys",
        Some("YOSYS"),
    )))
    .unwrap();
    let mut design = Design {
        files: vec![SourceFile::new(
            "rtl source/top.sv",
            "`include \"width.vh\"\nmodule top #(parameter N=1)(input logic [`WIDTH-1:0] x, output logic [N-1:0] y); assign y=x; endmodule",
        )],
        headers: vec![SourceFile::new(
            "include dir/width.vh",
            "`define WIDTH `INPUT_WIDTH",
        )],
        top: Some("top".into()),
        ..Design::default()
    };
    design.options.system_verilog = true;
    design
        .options
        .defines
        .insert("INPUT_WIDTH".into(), Some("8".into()));
    design.options.include_dirs.push("include dir".into());
    design.parameters.insert("N".into(), "8".into());
    let mut commands = hierarchy_commands(&design).unwrap();
    commands.extend([
        "select -assert-count 2 top/w:x top/w:y".into(),
        "write_rtlil result.il".into(),
    ]);
    let mut job = YosysJob {
        design,
        ..YosysJob::default()
    };
    job.stage("frontend", commands).unwrap();
    job.stage("stat", vec!["stat -json".into()]).unwrap();
    let prepared = runner.prepare(&job, &mut context).unwrap();
    let result = runner.measure(&prepared, "frontend", &mut context).unwrap();
    let rtl = fs::read_to_string(prepared.directory.path().join("result.il")).unwrap();
    assert!(rtl.contains("wire width 8 input"), "{rtl}");
    assert!(rtl.contains("wire width 8 output"), "{rtl}");
    result.statistics("stat").unwrap();
    let script = prepared.directory.path().join("run.ys");
    fs::write(
        &script,
        "tee -o .yosys-logs/frontend.log log partial\nselect -assert-count 1 *\n",
    )
    .unwrap();
    assert!(matches!(
        runner.measure(&prepared, "failed", &mut context),
        Err(ExperimentError::Process { .. })
    ));
    assert_eq!(
        fs::read_to_string(context.workspace().join("logs/failed/frontend.log")).unwrap(),
        "partial\n"
    );
    fs::write(&script, "log no-stage-log\n").unwrap();
    assert!(matches!(
        runner.measure(&prepared, "missing", &mut context),
        Err(ExperimentError::Parse(_))
    ));
    assert!(result.statistics("stat").is_ok());
    fs::write(&script, "missing_custom_pass\n").unwrap();
    assert!(
        matches!(runner.measure(&prepared, "unsupported", &mut context),
        Err(ExperimentError::Unsupported(reason)) if reason.contains("missing_custom_pass"))
    );
    drop(prepared);
    assert!(!fs::read_dir(context.workspace()).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".job-")
    }));
}
