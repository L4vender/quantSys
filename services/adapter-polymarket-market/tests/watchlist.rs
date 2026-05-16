use adapter_polymarket_market::watchlist::market_assets_for_watchlist;
use quantsys_domain::WsWatchlist;
use serde_json::json;

#[test]
fn watchlist_provides_polymarket_assets_for_market_subscription() {
    let watchlist: WsWatchlist = serde_json::from_value(json!({
        "schema_version": "quantsys.ws_watchlist.v1",
        "generated_at": "2026-05-15T14:00:00Z",
        "items": [{
            "canonical_event_id": "nba:lakers_at_celtics:2026-05-16",
            "canonical_market_key": "nba:lakers_at_celtics:2026-05-16:full_game:moneyline:na",
            "sport": "nba",
            "league": "nba",
            "event_name": "Los Angeles Lakers vs Boston Celtics",
            "event_start_time_utc": "2026-05-16T00:00:00Z",
            "market_type": "moneyline",
            "period": "full_game",
            "line": null,
            "therundown_event_id": "tr_lal_bos",
            "therundown_market_id": "1",
            "polymarket_event_id": "pm_lal_bos",
            "polymarket_condition_id": "pm_condition_moneyline",
            "polymarket_market_id": "pm_market_moneyline",
            "polymarket_asset_ids": ["pm_yes", "pm_no"]
        }]
    }))
    .unwrap();

    assert_eq!(
        market_assets_for_watchlist(&watchlist).unwrap(),
        vec!["pm_yes", "pm_no"]
    );
}
