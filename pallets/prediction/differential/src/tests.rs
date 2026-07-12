//! Differential baseline integration tests.
//! 差分基线集成测试。

use crate::{compare::assert_snapshot_eq, goldens::goldens, scenarios::scenario_catalog};

#[test]
fn differential_baseline_pins_upstream_commit() {
    assert_eq!(scenario_catalog()[0].name, "permissionless_resolve_native");
    assert_eq!(
        crate::scenarios::UPSTREAM_BASELINE_COMMIT,
        "39ad8d60aa2f7af0a465d58c5e87dcc509602df5"
    );
}

#[test]
fn differential_catalog_matches_upstream_goldens() {
    let expected_by_name = goldens()
        .into_iter()
        .map(|snapshot| (snapshot.name, snapshot))
        .collect::<std::collections::BTreeMap<_, _>>();

    for entry in scenario_catalog() {
        let actual = (entry.run)();
        let expected = expected_by_name
            .get(entry.name)
            .unwrap_or_else(|| panic!("missing golden for {:?}", entry.name));
        assert_snapshot_eq(&actual, expected);
    }
}

#[test]
fn differential_catalog_has_unique_names() {
    let mut names = std::collections::BTreeSet::new();
    for entry in scenario_catalog() {
        assert!(
            names.insert(entry.name),
            "duplicate scenario {:?}",
            entry.name
        );
    }
    assert_eq!(names.len(), scenario_catalog().len());
}
