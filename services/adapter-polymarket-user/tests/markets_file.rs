use adapter_polymarket_user::config::load_markets_file;

#[test]
fn loads_condition_ids_from_live_mapping_output() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("polymarket_user_markets.json");
    std::fs::write(
        &path,
        r#"{
          "condition_ids": ["0xspread", "0xtotal", "", "0xspread"],
          "source": "live_mapping_matched"
        }"#,
    )
    .unwrap();

    let ids = load_markets_file(&path).unwrap();

    assert_eq!(ids, vec!["0xspread".to_string(), "0xtotal".to_string()]);
}

#[test]
fn missing_markets_file_is_empty_not_error() {
    let dir = tempfile::tempdir().unwrap();

    let ids = load_markets_file(dir.path().join("missing.json")).unwrap();

    assert!(ids.is_empty());
}
