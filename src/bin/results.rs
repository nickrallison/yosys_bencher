//! JSON access to the same backend the UI will use.
use serde::{Serialize, de::DeserializeOwned};
use std::{
    fs::File,
    io::{self, BufReader},
    path::PathBuf,
};
use yosys_bencher::{
    analysis::{AnalysisPlan, Backend, InputRequest, InspectionRequest, Paging},
    store::{ArtifactId, ExperimentId, RevisionQuery, RunId, SqliteStore, WorkItemId},
};

fn read<T: DeserializeOwned>(path: &str) -> Result<T, Box<dyn std::error::Error>> {
    if path == "-" {
        Ok(serde_json::from_reader(io::stdin().lock())?)
    } else {
        Ok(serde_json::from_reader(BufReader::new(File::open(path)?))?)
    }
}
fn print(value: &impl Serialize) -> Result<(), Box<dyn std::error::Error>> {
    serde_json::to_writer_pretty(io::stdout().lock(), value)?;
    println!();
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    let path = if args.first().is_some_and(|s| s == "--database") {
        if args.len() < 2 {
            return Err("--database requires a path".into());
        }
        let path = PathBuf::from(args.remove(1));
        args.remove(0);
        path
    } else {
        SqliteStore::default_path()
    };
    if args.first().is_some_and(|s| s == "--help" || s == "-h") {
        println!(
            "results [--database PATH] [list | describe ID | inputs REQUEST.json | groups REQUEST.json FIELD | revisions ID | runs ID | analyze PLAN.json | inspect REQUEST.json | run ID | attempts WORK_ID | artifact ID OUTPUT]\nUse - to read JSON from stdin."
        );
        return Ok(());
    }
    let backend = Backend::open(path)?;
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] | ["list"] => print(&backend.experiments()?),
        ["describe", id] => print(&backend.describe(ExperimentId(id.parse()?))?),
        ["inputs", path] => {
            let r: InputRequest = read(path)?;
            print(&backend.inputs(&r.selection, &r.search, r.paging)?)
        }
        ["groups", path, field] => {
            let r: InputRequest = read(path)?;
            print(&backend.groups(&r.selection, field)?)
        }
        ["revisions", id] => {
            print(&backend.revisions(ExperimentId(id.parse()?), &RevisionQuery::All)?)
        }
        ["runs", id] => print(&backend.runs(ExperimentId(id.parse()?))?),
        ["analyze", path] => {
            let plan: AnalysisPlan = read(path)?;
            print(&backend.analyze(&plan)?)
        }
        ["inspect", path] => {
            let r: InspectionRequest = read(path)?;
            print(&backend.inspect(&r.snapshot, &r.row, r.paging)?)
        }
        ["run", id] => print(&backend.inspect_run(RunId(id.parse()?))?),
        ["attempts", id] => {
            print(&backend.inspect_work_item(WorkItemId(id.parse()?), Paging::default())?)
        }
        ["artifact", id, path] => {
            std::fs::write(path, backend.read_artifact(ArtifactId(id.parse()?))?)?;
            Ok(())
        }
        _ => Err("Unknown command; use results --help".into()),
    }
}
