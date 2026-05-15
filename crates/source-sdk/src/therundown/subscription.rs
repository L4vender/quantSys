use crate::therundown::error::{ApiKey, TheRundownError};
use url::Url;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SubscriptionFilters {
    pub sport_ids: Vec<u32>,
    pub market_ids: Vec<u32>,
    pub affiliate_ids: Vec<u32>,
    pub event_ids: Vec<String>,
}

impl SubscriptionFilters {
    pub fn has_any_filter(&self) -> bool {
        !self.sport_ids.is_empty()
            || !self.market_ids.is_empty()
            || !self.affiliate_ids.is_empty()
            || !self.event_ids.is_empty()
    }

    pub fn validate(&self, filters_required: bool) -> Result<(), TheRundownError> {
        if filters_required && !self.has_any_filter() {
            return Err(TheRundownError::Config(
                "TheRundown production websocket subscriptions require at least one filter"
                    .to_string(),
            ));
        }
        Ok(())
    }
}

pub fn build_ws_url(
    ws_url: &str,
    api_key: &ApiKey,
    filters: &SubscriptionFilters,
) -> Result<String, TheRundownError> {
    let mut url = Url::parse(ws_url).map_err(|err| TheRundownError::Config(err.to_string()))?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("key", api_key.expose_for_transport());
        append_u32_filter(&mut pairs, "sport_ids", &filters.sport_ids);
        append_u32_filter(&mut pairs, "market_ids", &filters.market_ids);
        append_u32_filter(&mut pairs, "affiliate_ids", &filters.affiliate_ids);
        if !filters.event_ids.is_empty() {
            pairs.append_pair("event_ids", &filters.event_ids.join(","));
        }
    }
    Ok(url.to_string())
}

pub fn redact_ws_url(url: &str) -> String {
    let Ok(mut parsed) = Url::parse(url) else {
        return url.to_string();
    };
    let pairs = parsed
        .query_pairs()
        .map(|(key, value)| {
            if key == "key" {
                (key.to_string(), "<redacted>".to_string())
            } else {
                (key.to_string(), value.to_string())
            }
        })
        .collect::<Vec<_>>();
    parsed.set_query(None);
    {
        let mut query = parsed.query_pairs_mut();
        for (key, value) in pairs {
            query.append_pair(&key, &value);
        }
    }
    parsed.to_string()
}

fn append_u32_filter(
    pairs: &mut url::form_urlencoded::Serializer<'_, url::UrlQuery<'_>>,
    name: &str,
    values: &[u32],
) {
    if !values.is_empty() {
        pairs.append_pair(
            name,
            &values
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(","),
        );
    }
}
