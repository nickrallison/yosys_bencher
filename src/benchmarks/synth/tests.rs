use super::*;
use crate::{
    experiment::{CmpSuite, ProcedureId, SuiteDefinition, Version},
    store::{ExperimentSpec, RunPlan, SqliteStore},
};
use std::collections::HashSet;

#[test]
fn named_library_identity_ignores_cell_contents_but_retains_them_for_execution() {
    let original = CellLibrary::new(format!(
        "{}library (demo) {{ cell (old) {{}} }}",
        "/* header */\n".repeat(36)
    ));
    let changed = CellLibrary::new(" library (\"demo\") { cell (new) {} }");
    let other = CellLibrary::new("library (other) { cell (old) {} }");
    assert_eq!(original.name(), Some("demo"));
    assert_eq!(original, changed);
    assert_eq!(HashSet::from([original, changed.clone(), other]).len(), 2);
    let json = serde_json::to_string(&changed).unwrap();
    let restored: CellLibrary = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.liberty(), changed.liberty());
    assert_eq!(
        SynthConfig::new(changed, vec![]).parameters()["library"],
        "demo"
    );
    assert_ne!(CellLibrary::new("invalid A"), CellLibrary::new("invalid B"));
}

#[test]
fn same_named_library_reuses_stored_input_and_suite_version() {
    type P = YosysBenchmark<SynthProcedure>;
    let definition = |library: &str| {
        SuiteDefinition::new(
            ProcedureId::new("yosys.synth", Version::default()),
            vec![SynthInput::new(
                design::Design::default(),
                SynthConfig::new(CellLibrary::new(library), vec![]),
            )],
        )
        .unwrap()
    };
    let original = definition("library (demo) { cell (old) {} }");
    let changed = definition("library (demo) { cell (new) {} }");
    let different = definition("library (other) { cell (old) {} }");
    assert_eq!(original.compare(&changed), CmpSuite::Eq);
    assert_eq!(original.compare(&different), CmpSuite::Incompatible);
    let mut store = SqliteStore::in_memory().unwrap();
    let experiment = ExperimentSpec {
        key: "library-identity".into(),
        name: "Library identity".into(),
        description: String::new(),
        repository: "test".into(),
    };
    let plan = RunPlan::new(vec![], 1);
    let first = store
        .register_run::<P>(&experiment, &original, &plan)
        .unwrap();
    let second = store
        .register_run::<P>(&experiment, &changed, &plan)
        .unwrap();
    assert_eq!(first.inputs, second.inputs);
    assert_eq!(first.definition, second.definition);
    let third = store
        .register_run::<P>(&experiment, &different, &plan)
        .unwrap();
    assert_ne!(first.inputs, third.inputs);
}

#[test]
fn unnamed_library_warning_reaches_analysis_results() {
    use crate::{
        analysis::{AnalysisPlan, Backend},
        store::ResultSelection,
    };
    type P = YosysBenchmark<SynthProcedure>;
    let input = SynthInput::new(
        design::Design::default(),
        SynthConfig::new(CellLibrary::new("cell (demo) {}"), vec![]),
    );
    assert_eq!(input.warnings().len(), 1);
    let definition = SuiteDefinition::new(
        ProcedureId::new("yosys.synth", Version::default()),
        vec![input],
    )
    .unwrap();
    let mut store = SqliteStore::in_memory().unwrap();
    let run = store
        .register_run::<P>(
            &ExperimentSpec {
                key: "unnamed".into(),
                name: "Unnamed".into(),
                description: String::new(),
                repository: "test".into(),
            },
            &definition,
            &RunPlan::new(vec![], 1),
        )
        .unwrap();
    let result = Backend::new(store)
        .unwrap()
        .analyze(&AnalysisPlan::history(
            ResultSelection::new(run.experiment),
            "runtime",
        ))
        .unwrap();
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.contains("no declared name"))
    );
    assert!(
        SynthConfig::new(CellLibrary::new("library (demo) {}"), vec![])
            .warnings()
            .is_empty()
    );
}
