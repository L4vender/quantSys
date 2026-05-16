use quantsys_domain::WsWatchlist;
use serde_json::json;

#[test]
fn watchlist_extracts_ws_subscription_ids() {
    let watchlist: WsWatchlist = serde_json::from_value(json!({
        "schema_version": "quantsys.ws_watchlist.v1",
        "generated_at": "2026-05-15T14:00:00Z",
        "items": [
            {
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
                "polymarket_asset_ids": ["pm_yes", "pm_no"],
                "selection_reason": "max_market_count",
                "matched_market_count": 1
            },
            {
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
                "polymarket_asset_ids": ["pm_spread_yes", "pm_spread_no"],
                "selection_reason": "median_line_tie_break",
                "matched_market_count": 2
            }
        ],
        "therundown": {
            "event_ids": ["tr_lal_bos"],
            "market_ids": [1, 2]
        },
        "polymarket": {
            "condition_ids": ["pm_condition_moneyline", "pm_condition_spread"],
            "asset_ids": ["pm_yes", "pm_no", "pm_spread_yes", "pm_spread_no"]
        }
    }))
    .unwrap();

    assert_eq!(watchlist.therundown_event_ids(), vec!["tr_lal_bos"]);
    assert_eq!(watchlist.therundown_market_ids(), vec![1, 2]);
    assert_eq!(
        watchlist.polymarket_asset_ids(),
        vec!["pm_yes", "pm_no", "pm_spread_yes", "pm_spread_no"]
    );
    assert!(watchlist.allows_therundown_market_price("tr_lal_bos", 1, None));
    assert!(watchlist.allows_therundown_market_price("tr_lal_bos", 2, Some(2.5)));
    assert!(!watchlist.allows_therundown_market_price("tr_lal_bos", 2, Some(3.5)));
}
