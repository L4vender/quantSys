use crate::therundown::error::{ApiKey, TheRundownError};
use crate::therundown::headers::EntitlementHeaders;
use async_trait::async_trait;
use serde_json::Value;
use std::time::Duration;
use url::Url;

#[derive(Clone, Debug, PartialEq)]
pub struct MockRestResponse {
    pub status: u16,
    pub headers: EntitlementHeaders,
    pub body: Value,
}

impl MockRestResponse {
    pub fn new<I, K, V>(status: u16, headers: I, body: Value) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        Self {
            status,
            headers: EntitlementHeaders::from_pairs(headers),
            body,
        }
    }
}

#[async_trait]
pub trait RestTransport: Clone + Send + Sync + 'static {
    async fn get_json(
        &self,
        url: &str,
        api_key: &ApiKey,
        timeout: Duration,
    ) -> Result<MockRestResponse, TheRundownError>;
}

#[derive(Clone, Debug, Default)]
pub struct ReqwestRestTransport {
    client: reqwest::Client,
}

impl ReqwestRestTransport {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl RestTransport for ReqwestRestTransport {
    async fn get_json(
        &self,
        url: &str,
        api_key: &ApiKey,
        timeout: Duration,
    ) -> Result<MockRestResponse, TheRundownError> {
        let response = self
            .client
            .get(url)
            .header(api_key.header_name(), api_key.expose_for_transport())
            .timeout(timeout)
            .send()
            .await
            .map_err(|err| TheRundownError::Transport(api_key.scrub(&err.to_string())))?;
        let status = response.status().as_u16();
        let headers = EntitlementHeaders::from_header_map(response.headers());
        let body = response
            .json::<Value>()
            .await
            .map_err(|err| TheRundownError::MalformedJson(api_key.scrub(&err.to_string())))?;
        Ok(MockRestResponse {
            status,
            headers,
            body,
        })
    }
}

#[derive(Clone, Debug)]
pub struct TheRundownRestClient<T> {
    base_url: String,
    api_key: ApiKey,
    transport: T,
    timeout: Duration,
}

impl<T> TheRundownRestClient<T>
where
    T: RestTransport,
{
    pub fn new(
        base_url: impl Into<String>,
        api_key: ApiKey,
        transport: T,
        timeout: Duration,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            api_key,
            transport,
            timeout,
        }
    }

    pub async fn probe(&self) -> Result<MockRestResponse, TheRundownError> {
        let url = build_probe_url(&self.base_url)?;
        self.get_checked(&url).await
    }

    pub async fn events_bootstrap(
        &self,
        sport_id: u32,
        date: &str,
    ) -> Result<MockRestResponse, TheRundownError> {
        let url = build_events_bootstrap_url(&self.base_url, sport_id, date)?;
        self.get_checked(&url).await
    }

    pub async fn markets_delta(&self, last_id: &str) -> Result<MockRestResponse, TheRundownError> {
        let url = build_markets_delta_url(&self.base_url, last_id)?;
        self.get_checked(&url).await
    }

    async fn get_checked(&self, url: &str) -> Result<MockRestResponse, TheRundownError> {
        let response = self
            .transport
            .get_json(url, &self.api_key, self.timeout)
            .await?;
        interpret_response(response)
    }
}

pub fn build_probe_url(base_url: &str) -> Result<String, TheRundownError> {
    let base = base_url.trim_end_matches('/');
    Ok(format!("{base}/sports"))
}

pub fn build_events_bootstrap_url(
    base_url: &str,
    sport_id: u32,
    date: &str,
) -> Result<String, TheRundownError> {
    let base = base_url.trim_end_matches('/');
    Ok(format!("{base}/sports/{sport_id}/events/{date}"))
}

pub fn build_markets_delta_url(base_url: &str, last_id: &str) -> Result<String, TheRundownError> {
    let mut url = Url::parse(&format!("{}/markets/delta", base_url.trim_end_matches('/')))
        .map_err(|err| TheRundownError::Config(err.to_string()))?;
    url.query_pairs_mut().append_pair("last_id", last_id);
    Ok(url.to_string())
}

pub fn interpret_response(response: MockRestResponse) -> Result<MockRestResponse, TheRundownError> {
    match response.status {
        200..=299 => Ok(response),
        401 => Err(TheRundownError::AuthFailed),
        409 | 410 => Err(TheRundownError::CursorStale),
        429 => Err(TheRundownError::RateLimited {
            retry_after: response.headers.retry_after,
        }),
        status if status >= 500 => Err(TheRundownError::Server { status }),
        status => Err(TheRundownError::Transport(format!(
            "TheRundown returned unexpected status {status}"
        ))),
    }
}
