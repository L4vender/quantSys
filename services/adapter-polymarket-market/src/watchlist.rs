use anyhow::{bail, Context};
use quantsys_domain::WsWatchlist;
use std::path::Path;

pub fn load_watchlist(path: impl AsRef<Path>) -> anyhow::Result<WsWatchlist> {
    WsWatchlist::load_from_path(path.as_ref())
        .with_context(|| format!("loading websocket watchlist {}", path.as_ref().display()))
}

pub fn market_assets_for_watchlist(watchlist: &WsWatchlist) -> anyhow::Result<Vec<String>> {
    if watchlist.is_empty() {
        bail!("websocket watchlist has no matched market items");
    }
    let asset_ids = watchlist.polymarket_asset_ids();
    if asset_ids.is_empty() {
        bail!("websocket watchlist is missing Polymarket asset ids");
    }
    Ok(asset_ids)
}
