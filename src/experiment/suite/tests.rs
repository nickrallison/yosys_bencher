use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct Input(i64);
impl BenchmarkIn for Input {}

fn definition(name: &str, values: &[i64], version: Version) -> SuiteDefinition<Input> {
    SuiteDefinition::new(
        ProcedureId::new(name, version),
        values.iter().copied().map(Input).collect(),
    )
    .unwrap()
}

#[test]
fn suite_identity_and_version_follow_membership_while_preserving_execution_order() {
    let previous = definition("test", &[1, 2], Version::new(2, 3));
    for (name, values, relation, (major, minor)) in [
        ("test", vec![2, 1], CmpSuite::Eq, (2, 3)),
        ("test", vec![3, 1, 2], CmpSuite::Superset, (2, 4)),
        ("test", vec![1], CmpSuite::Incompatible, (3, 0)),
        ("test", vec![1, 3], CmpSuite::Incompatible, (3, 0)),
        ("renamed", vec![1, 2], CmpSuite::Incompatible, (3, 0)),
    ] {
        let current = definition(name, &values, Version::default());
        assert_eq!(current.compare(&previous), relation);
        let resolved = current.with_previous(&previous);
        assert_eq!(resolved.version(), &Version::new(major, minor));
        assert_eq!(resolved.compare(&previous), relation);
        assert_eq!(
            resolved.inputs(),
            &values.into_iter().map(Input).collect::<Vec<_>>()
        );
        let repeated = resolved.clone().with_previous(&resolved);
        assert_eq!(repeated.compare(&resolved), CmpSuite::Eq);
        assert_eq!(repeated.version(), resolved.version());
    }
}

#[test]
fn suite_construction_and_deserialization_enforce_unique_inputs() {
    let original = definition("test", &[1, 2], Version::new(2, 3));
    let json = serde_json::to_string(&original).unwrap();
    let restored: SuiteDefinition<Input> = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.procedure(), original.procedure());
    assert_eq!(restored.inputs(), original.inputs());
    assert!(matches!(
        SuiteDefinition::new(original.procedure().clone(), vec![Input(1); 2]),
        Err(ExperimentError::DuplicateInput { index: 1 })
    ));
    let mut json = serde_json::to_value(&original).unwrap();
    json["inputs"] = serde_json::json!([1, 1]);
    assert!(serde_json::from_value::<SuiteDefinition<Input>>(json).is_err());
}
