use quantsys_eventbus::{TopicCatalog, TopicRetention};

#[test]
fn default_topic_catalog_contains_phase2_bus_metadata() {
    let catalog = TopicCatalog::phase2_default();
    let raw = catalog.get("raw.therundown").unwrap();

    assert_eq!(raw.key, "provider_event_id");
    assert_eq!(raw.partitions, 3);
    assert_eq!(raw.retention, TopicRetention::Days(14));
    assert!(catalog.get("norm.quote").is_some());
    assert!(catalog.get("dlq.raw").is_some());
}

#[test]
fn topic_catalog_parses_topic_init_toml() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../scripts/topic-init/topics.toml"
    );
    let catalog = TopicCatalog::from_toml_file(path).unwrap();

    assert_eq!(
        catalog.get("raw.polymarket.market").unwrap().producer,
        "adapter-polymarket-market"
    );
    assert_eq!(
        catalog.get("execution.receipt").unwrap().retention,
        TopicRetention::Days(365)
    );
}
