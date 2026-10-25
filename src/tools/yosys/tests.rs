use super::*;
use crate::experiment::ArtifactPolicy;

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
fn preparation_stages_ordered_files_options_and_headers() {
    let root = tempfile::tempdir().unwrap();
    let mut context = RunContext::new(root.path().into(), ArtifactPolicy::new(false, false, false));
    let runner = YosysRunner {
        executable: "/bin/true".into(),
    };
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
    let prepared = runner.prepare(&job, &mut context).unwrap();
    assert!(
        prepared
            .directory
            .path()
            .join("include dir/header.vh")
            .is_file()
    );
    assert!(prepared.script.contains("-sv"));
    assert!(prepared.script.contains("-DWIDTH=8"));
    assert!(prepared.script.contains("-I.yosys-includes/0"));
    assert!(prepared.script.contains("\"rtl/top.v\" \"second file.sv\""));
    assert!(!prepared.script.contains("header.vh"));
    assert!(!prepared.script.contains("write_verilog"));
    drop(prepared);
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn preparation_rejects_escaping_and_conflicting_paths() {
    let root = tempfile::tempdir().unwrap();
    let mut context = RunContext::new(root.path().into(), ArtifactPolicy::new(false, false, false));
    let runner = YosysRunner {
        executable: "/bin/true".into(),
    };
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
        assert!(runner.prepare(&job, &mut context).is_err(), "{path}");
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    }
}

#[test]
fn stage_logs_are_fresh_per_execution_and_failures_keep_partial_logs() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().unwrap();
    let executable = root.path().join("yosys");
    fs::write(
        &executable,
        "#!/bin/sh\necho first > .yosys-logs/stage.log\n",
    )
    .unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    let runner = YosysRunner::new(&Yosys::new(&executable)).unwrap();
    let mut context = RunContext::new(
        root.path().join("attempt"),
        ArtifactPolicy::new(true, false, false),
    );
    let mut job = job();
    job.stage("stage", vec!["stat".into()]).unwrap();
    let prepared = runner.prepare(&job, &mut context).unwrap();
    let first = runner.measure(&prepared, "first", &mut context).unwrap();
    assert_eq!(first.log("stage").unwrap(), "first\n");

    fs::write(
        &executable,
        "#!/bin/sh\necho partial > .yosys-logs/stage.log\nexit 9\n",
    )
    .unwrap();
    assert!(matches!(
        runner.measure(&prepared, "failed", &mut context),
        Err(ExperimentError::Process { .. })
    ));
    assert_eq!(
        fs::read_to_string(context.workspace().join("logs/failed/stage.log")).unwrap(),
        "partial\n"
    );

    fs::write(&executable, "#!/bin/sh\nexit 0\n").unwrap();
    assert!(matches!(
        runner.measure(&prepared, "missing", &mut context),
        Err(ExperimentError::Parse(_))
    ));
    assert_eq!(first.log("stage").unwrap(), "first\n");
}

#[test]
fn statistics_count_leaf_cells_across_the_whole_hierarchy() {
    let text = r#"{"modules":{"\\top":{"num_cells":0,"num_cells_by_type":{"child":2}},"\\child":{"num_cells":1,"num_cells_by_type":{"NAND2_X1":1}}},"design":{"num_cells":2,"area":84.588,"num_cells_by_type":{"child":2,"NAND2_X1":2}}}"#;
    let stats = Statistics::parse(text).unwrap();
    assert_eq!(stats.cell_count, HashMap::from([("NAND2_X1".into(), 2)]));
    assert_eq!(stats.area, Some(84.588));
    // Historical stat JSON counted module instances in num_cells.
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
#[ignore = "requires a real Yosys executable in YOSYS"]
fn real_frontend_handles_filelists_includes_defines_and_parameters() {
    let root = tempfile::tempdir().unwrap();
    let mut context = RunContext::new(root.path().into(), ArtifactPolicy::default());
    let runner =
        YosysRunner::new(&Yosys::new(std::env::var_os("YOSYS").expect("set YOSYS"))).unwrap();
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
}
