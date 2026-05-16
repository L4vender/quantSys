use adapter_therundown::watchlist::{
    filters_for_watchlist, therundown_market_price_allowed_by_watchlist,
};
use quantsys_domain::WsWatchlist;
use quantsys_source_sdk::therundown::SubscriptionFilters;
use serde_json::json;

fn watchlist() -> WsWatchlist {
    serde_json::from_value(json!({
        "schema_version": "quantsys.ws_watchlist.v1",
        "generated_at": "2026-05-15T14:00:00Z",
        "items": [{
            "canonical_event_id": "nba:lakers_at_celtics:2026-05-16",
            "canonical_market_key": "nba:lakers_at_celtics:2026-05-16:full_game:spread:2p5",
            "sport": "nba",
            "league": "nba",
            "event_name": "Los Angeles Lakers vs Boston Celtics",
            "event_start_time_utc": "2026-05-16T00:00:00Z",
            "market_type": "spread",
            "period": "full_game",
            "line": 2.5,
            "therundown_event_id": "tr_lal_bos",
            "therundown_market_id": "2",
            "polymarket_event_id": "pm_lal_bos",
            "polymarket_condition_id": "pm_condition_spread",
            "polymarket_market_id": "pm_market_spread",
            "polymarket_asset_ids": ["pm_spread_yes", "pm_spread_no"]
        }]
    }))
    .unwrap()
}

#[test]
fn watchlist_overrides_therundown_event_and_market_filters_but_keeps_affiliates() {
    let base = SubscriptionFilters {
        sport_ids: vec![4],
        market_ids: vec![1, 2, 3],
        affiliate_ids: vec![19, 23],
        event_ids: vec![],
    };

    let filters = filters_for_watchlist(base, &watchlist()).unwrap();

    assert_eq!(filters.sport_ids, vec![4]);
    assert_eq!(filters.market_ids, vec![2]);
    assert_eq!(filters.affiliate_ids, vec![19, 23]);
    assert_eq!(filters.event_ids, vec!["tr_lal_bos"]);
}

#[test]
fn watchlist_filters_therundown_ws_payload_by_event_market_and_line() {
    let allowed = json!({
        "meta": {"type": "market_price"},
        "data": {"event_id": "tr_lal_bos", "market_id": 2, "line": "-2.5"}
    });
    let wrong_line = json!({
        "meta": {"type": "market_price"},
        "data": {"event_id": "tr_lal_bos", "market_id": 2, "line": "-3.5"}
    });
    let heartbeat = json!({"meta": {"type": "heartbeat"}});

    assert!(therundown_market_price_allowed_by_watchlist(
        &watchlist(),
        &allowed
    ));
    assert!(!therundown_market_price_allowed_by_watchlist(
        &watchlist(),
        &wrong_line
    ));
    assert!(therundown_market_price_allowed_by_watchlist(
        &watchlist(),
        &heartbeat
    ));
}
