use anyhow::{bail, Context};
use std::env;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::handshake::client::Response;
use tokio_tungstenite::{
    client_async_tls_with_config, connect_async, MaybeTlsStream, WebSocketStream,
};
use url::Url;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HttpProxy {
    host: String,
    port: u16,
}

impl HttpProxy {
    fn authority(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

pub async fn connect_ws_with_optional_proxy(
    url: &str,
) -> anyhow::Result<(WebSocketStream<MaybeTlsStream<TcpStream>>, Response)> {
    if let Some(proxy) = configured_http_proxy()? {
        tracing::info!(
            proxy_host = %proxy.host,
            proxy_port = proxy.port,
            "using HTTP CONNECT proxy for Polymarket market websocket"
        );
        return connect_ws_via_http_proxy(url, &proxy).await;
    }

    connect_async(url)
        .await
        .with_context(|| format!("connecting websocket {url}"))
}

fn configured_http_proxy() -> anyhow::Result<Option<HttpProxy>> {
    for key in [
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
        "HTTP_PROXY",
        "http_proxy",
    ] {
        if let Ok(value) = env::var(key) {
            if value.trim().is_empty() {
                continue;
            }
            return parse_http_proxy_url(&value).map(Some);
        }
    }
    Ok(None)
}

async fn connect_ws_via_http_proxy(
    ws_url: &str,
    proxy: &HttpProxy,
) -> anyhow::Result<(WebSocketStream<MaybeTlsStream<TcpStream>>, Response)> {
    let target = Url::parse(ws_url).with_context(|| format!("parsing websocket URL {ws_url}"))?;
    let host = target
        .host_str()
        .context("websocket URL must include host")?
        .to_string();
    let port = target.port_or_known_default().with_context(|| {
        format!(
            "websocket URL must include a port or known scheme default: {}",
            target.scheme()
        )
    })?;

    let mut stream = TcpStream::connect(proxy.authority())
        .await
        .with_context(|| format!("connecting to HTTP proxy {}:{}", proxy.host, proxy.port))?;
    stream
        .write_all(http_connect_request(&host, port)?.as_bytes())
        .await
        .context("sending HTTP CONNECT request for Polymarket market websocket")?;

    read_http_connect_response(&mut stream)
        .await
        .context("reading HTTP CONNECT response for Polymarket market websocket")?;

    client_async_tls_with_config(ws_url, stream, None, None)
        .await
        .with_context(|| format!("performing websocket handshake through HTTP proxy for {ws_url}"))
}

pub(crate) fn parse_http_proxy_url(value: &str) -> anyhow::Result<HttpProxy> {
    let normalized = if value.contains("://") {
        value.to_string()
    } else {
        format!("http://{value}")
    };
    let url = Url::parse(&normalized)
        .with_context(|| format!("parsing HTTP proxy URL from value {value:?}"))?;
    if url.scheme() != "http" {
        bail!("only HTTP proxy URLs are supported for Polymarket market websocket");
    }
    if !url.username().is_empty() || url.password().is_some() {
        bail!("authenticated proxy URLs are not supported to avoid credential logging risk");
    }
    let host = url
        .host_str()
        .context("HTTP proxy URL must include host")?
        .to_string();
    let port = url.port_or_known_default().unwrap_or(80);
    Ok(HttpProxy { host, port })
}

pub(crate) fn http_connect_request(host: &str, port: u16) -> anyhow::Result<String> {
    if host.contains('\r') || host.contains('\n') {
        bail!("invalid websocket host for HTTP CONNECT");
    }
    Ok(format!(
        "CONNECT {host}:{port} HTTP/1.1\r\nHost: {host}:{port}\r\nProxy-Connection: Keep-Alive\r\n\r\n"
    ))
}

async fn read_http_connect_response(stream: &mut TcpStream) -> anyhow::Result<()> {
    let mut response = Vec::with_capacity(512);
    let mut chunk = [0_u8; 256];
    loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            bail!("HTTP proxy closed before CONNECT response completed");
        }
        response.extend_from_slice(&chunk[..read]);
        if response.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
        if response.len() > 8192 {
            bail!("HTTP CONNECT response header exceeded 8KiB");
        }
    }

    let response_text = String::from_utf8_lossy(&response);
    let status_line = response_text.lines().next().unwrap_or_default();
    if !status_line.contains(" 200 ") {
        bail!("HTTP CONNECT proxy rejected websocket tunnel: {status_line}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_http_proxy_with_default_scheme_and_port() {
        let proxy = parse_http_proxy_url("127.0.0.1:6244").expect("proxy should parse");

        assert_eq!(proxy.host, "127.0.0.1");
        assert_eq!(proxy.port, 6244);
    }

    #[test]
    fn connect_request_targets_websocket_authority_not_proxy_authority() {
        let request =
            http_connect_request("ws-subscriptions-clob.polymarket.com", 443).expect("request");

        assert!(
            request.starts_with("CONNECT ws-subscriptions-clob.polymarket.com:443 HTTP/1.1\r\n")
        );
        assert!(request.contains("Host: ws-subscriptions-clob.polymarket.com:443\r\n"));
        assert!(!request.contains("127.0.0.1:6244"));
    }

    #[test]
    fn rejects_socks_proxy_for_market_websocket() {
        let error = parse_http_proxy_url("socks5h://127.0.0.1:6244")
            .expect_err("SOCKS proxy should be explicit unsupported");

        assert!(error.to_string().contains("only HTTP proxy"));
    }
}
