use crate::polymarket::error::{L2Credentials, PolymarketError};
use serde_json::{json, Value};

pub fn build_market_subscription_payload(
    asset_ids: &[String],
    custom_feature_enabled: bool,
) -> Result<Value, PolymarketError> {
    if asset_ids.is_empty() {
        return Err(PolymarketError::Subscription(
            "market subscription requires at least one assets_ids entry".to_string(),
        ));
    }
    let payload = json!({
        "assets_ids": asset_ids,
        "type": "market",
        "custom_feature_enabled": custom_feature_enabled,
    });
    validate_market_subscription_payload(&payload)?;
    Ok(payload)
}

pub fn validate_market_subscription_payload(payload: &Value) -> Result<(), PolymarketError> {
    if payload.get("asset_ids").is_some() {
        return Err(PolymarketError::Subscription(
            "market channel subscription must use assets_ids, not asset_ids".to_string(),
        ));
    }
    let assets = payload
        .get("assets_ids")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            PolymarketError::Subscription(
                "market channel subscription requires assets_ids".to_string(),
            )
        })?;
    if assets.is_empty() {
        return Err(PolymarketError::Subscription(
            "market channel subscription assets_ids cannot be empty".to_string(),
        ));
    }
    if payload.get("type").and_then(Value::as_str) != Some("market") {
        return Err(PolymarketError::Subscription(
            "market channel subscription type must be market".to_string(),
        ));
    }
    Ok(())
}

pub fn build_user_subscription_payload(
    auth: &L2Credentials,
    markets: &[String],
) -> Result<Value, PolymarketError> {
    if markets.is_empty() {
        return Err(PolymarketError::Subscription(
            "user subscription requires at least one markets condition id".to_string(),
        ));
    }
    Ok(json!({
        "auth": {
            "apiKey": auth.api_key(),
            "secret": auth.secret(),
            "passphrase": auth.passphrase(),
        },
        "markets": markets,
        "type": "user",
    }))
}
