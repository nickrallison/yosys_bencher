use super::*;
use crate::experiment::ArtifactPolicy;

fn job() -> YosysJob {
    YosysJob {
        design: Design {
            files: vec![SourceFile::new("rtl/top.v", "module top; endmodule")],
            ..Design::default()
        },
        assets: Vec::new(),
        commands: vec!["stat".into()],
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
    ] {
        let mut job = job();
        job.assets.push(SourceFile::new(path, "conflict"));
        assert!(runner.prepare(&job, &mut context).is_err(), "{path}");
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    }
}

#[test]
fn statistics_select_final_top_and_support_old_and_current_tables() {
    let old = "=== child ===\nNumber of cells: 1\n 1 $mul\n=== arbitrary ===\nNumber of cells: 3\n 1 $add\n 2 $mul\n";
    let current = "=== arbitrary ===\n3 - cells\n1 - $add\n1 - $sub\n1 - $neg\nChip area for top module 'arbitrary': 1.25e2\n";
    let stats = Statistics::parse(&format!("{old}{current}"), Some("arbitrary")).unwrap();
    assert_eq!(stats.cell_count.get("$mul"), None);
    assert_eq!(stats.cell_count.get("$sub"), Some(&1));
    assert_eq!(stats.area, Some(125.0));
    assert_eq!(
        Statistics::parse(old, Some("arbitrary")).unwrap().area,
        None
    );
    assert_eq!(Statistics::parse(current, None).unwrap().top, "arbitrary");
    assert_eq!(
        Statistics::parse(&format!("Top module: \\arbitrary\n{old}"), None)
            .unwrap()
            .top,
        "arbitrary"
    );
    for broken in [
        "",
        "=== arbitrary ===\n",
        "=== arbitrary ===\n2 cells\n1 $add\n",
        "=== arbitrary ===\n1 cells\n1 $add\nChip area for module 'arbitrary': NaN\n",
    ] {
        assert!(Statistics::parse(broken, Some("arbitrary")).is_err());
    }
    assert!(Statistics::parse(old, None).is_err());
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
        "stat".into(),
    ]);
    let prepared = runner
        .prepare(
            &YosysJob {
                design,
                assets: Vec::new(),
                commands,
            },
            &mut context,
        )
        .unwrap();
    let result = runner.measure(&prepared, "frontend", &mut context).unwrap();
    let rtl = fs::read_to_string(prepared.directory.path().join("result.il")).unwrap();
    assert!(rtl.contains("wire width 8 input"), "{rtl}");
    assert!(rtl.contains("wire width 8 output"), "{rtl}");
    Statistics::parse(result.stdout().unwrap(), None)
        .unwrap_or_else(|error| panic!("{error}\n{}", result.stdout().unwrap()));
}
