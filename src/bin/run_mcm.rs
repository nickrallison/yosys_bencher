use std::{fs, path::PathBuf};
use yosys_bench::{
    benchmarks::{
        mcm::{CoefficientGenerator, CoefficientSets, McmSuite, PaperTool},
        synth::CellLibrary,
    },
    experiment::{AttemptOutcome, Experiment, RunOptions, YosysProvider},
    src_manager::{
        build::CcacheCMakeBuilder,
        git::{
            GitManager,
            history::{CommitId, RevisionSelection},
        },
    },
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output_dir = PathBuf::from("output/mcm");
    let suite = McmSuite::new(
        CoefficientSets::Generated(CoefficientGenerator::default()),
        CellLibrary::from_path("../yosys/tests/techmap/mcm/NangateOpenCellLibrary_typical.lib")?,
        PaperTool {
            executable: "../yosys/tests/techmap/mcm/synth/acm".into(),
        },
    );
    let builder = CcacheCMakeBuilder::new(output_dir.join("ccache"))
        .configure_arg("-DYOSYS_USE_BUNDLED_LIBS=ON")
        .configure_arg("-DYOSYS_ENABLE_UNIT_TESTS=OFF")
        .with_suppressed_stdout();
    let mut provider = YosysProvider {
        git: GitManager::new(output_dir.join("source"), "nick/mcm", None).with_remote("../yosys"),
        builder,
        output_dir: output_dir.join("builds"),
    };
    let experiment = Experiment::new(
        RevisionSelection::Explicit(default_revisions()),
        suite,
        output_dir,
    );
    let report = experiment.run(&mut provider, &RunOptions::default())?;
    let json = serde_json::to_string_pretty(&report)?;
    let path = report.directory.join("results.json");
    fs::write(&path, &json)?;
    eprintln!("Results: {}", path.display());
    println!("{json}");
    if !report.revision_failures.is_empty()
        || report
            .attempts
            .iter()
            .any(|attempt| !matches!(attempt.record.outcome, AttemptOutcome::Success(_)))
    {
        return Err("some MCM attempts failed or were unsupported; see results.json".into());
    }
    Ok(())
}

fn default_revisions() -> Vec<CommitId> {
    [
        "fb1a2fdae7a84e327d239aec6e637fca4f037ae7",
        "70bf41c29bef52fe513cf81552114b09ff325caa",
        "ec3ddf1adf8d66f15078356454a3c7e80f3d5c1c",
        "7d3b4d260376d72e2f731040a6e5c1fc50cfedc2",
        "6029ae358528532585509b747941c7a9b16eeb4d",
        "f444f7d942409fa4c673856b6e85f5a66af7344c",
        "293f668020c7024ad26863d31f09233695337b1d",
        "f41e296144b3a741d63409b7fb884eef56e6881c",
        "84d5e8a897b5468f7f4b985aee300255c4fc2c5b",
        "aa0e05f93a2a27a8e3a88c7acf012198a1376fc9",
        "f836a636e3d838828e0f721320b6f14d50d851cc",
        "b04205e564da25c0246fa518b7b3f5bdd5435ed8",
        "edf018257b2fda232979b6051c46152ad74b0ab5",
        "921ba6541c557fb4d07dcebc5b3b6a94832ad804",
        "49710f7834acb9c643d101f0b24bb4b1f42b4b68",
        "a1780f80e3d5e3bb0a7c701eb475fb7a24ccb1cb",
        "ac5d1cc04f14f371cbff32503d394e4ce257ba20",
        "421b0ed545191ccec5a14bd30be998561cf01957",
        "b4452ec0f95fe901513d303d39240129bae76e57",
        "102e29d3d1ed70a89395329d90b77d7ef6aa6a46",
        "5797a7c4c2833a5de64816ebf4f33347c670dc29",
        "c579a824c1a3cc96f07602cc63576431357f5a48",
        "d5fbbe2d74c556aca021fe4d85ee94218b015249",
        "07195e5af7c9399e1be49be3384b51308c86b671",
        "dd2741eb18147cc4266fcad1e92821fcc4e7008c",
        "14dba6a6a5ba30c0706cad2428b733955b4bd55b",
        "66281e952aec6c28bfe5f23ae8d4eb08fd740fe8",
        "5355fa19fb0f7fb78f6d17c0223b56da71f0c8b2",
        "ea4b19e8e44019ccbaaf681bd795b77a0efe73e9",
        "8d8437f6cffe723f1b6e3c6d90d7734a513a98fb",
        "481c9af00b5cbcc2d99898e3b0e3a6f5b29e337b",
        "962674201bc1aa17812276df86c43168583e4ed0",
        "c55eb05cb151378b50a26a67ef3dd98e29e81765",
        "d77d3ecf341fd806466296d8ad402cf7f95fa9f8",
        "f47cc3bfa0f4e412e5c0bc6dcd3761891f98a6f4",
    ]
    .into_iter()
    .map(|id| CommitId::new(id.into()))
    .collect()
}
