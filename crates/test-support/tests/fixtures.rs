use quantsys_test_support::{canonical_json_hash, load_external_fixture, load_manifest};

#[test]
fn loads_phase1_manifest_and_fixture_json() {
    let manifest = load_manifest().unwrap();
    assert_eq!(manifest.fixtures.len(), 21);

    let fixture = load_external_fixture("therundown/ws_market_price.json").unwrap();
    assert_eq!(fixture["meta"]["type"], "market_price");
}

#[test]
fn canonical_json_hash_is_stable_for_key_order() {
    let a = serde_json::json!({"b": 2, "a": 1});
    let b = serde_json::json!({"a": 1, "b": 2});

    assert_eq!(
        canonical_json_hash(&a).unwrap(),
        canonical_json_hash(&b).unwrap()
    );
}
