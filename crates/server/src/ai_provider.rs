use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use async_trait::async_trait;
use axum::http::StatusCode;
use reqwest::Url;
use serde_json::Value;
use thiserror::Error;
use url::Host;

const MAX_PROVIDER_RESPONSE_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderMethod {
    Get,
    Post,
}

#[derive(Debug)]
pub struct ProviderResponse {
    pub status: StatusCode,
    pub body: Value,
}

#[derive(Debug, Error)]
pub enum AiProviderTransportError {
    #[error("AI provider URL is invalid")]
    InvalidUrl,
    #[error("AI provider DNS resolution was rejected: {0}")]
    DnsResolutionRejected(String),
    #[error("AI provider connection failed: {message}")]
    ConnectionFailure { message: String, timeout: bool },
    #[error("AI provider response exceeded the configured size limit")]
    ResponseTooLarge,
    #[error("AI provider response was not valid JSON: {0}")]
    InvalidJson(String),
}

impl AiProviderTransportError {
    #[must_use]
    pub const fn is_timeout(&self) -> bool {
        matches!(self, Self::ConnectionFailure { timeout: true, .. })
    }

    fn connection_failure(error: &impl ToString, timeout: bool) -> Self {
        Self::ConnectionFailure {
            message: error.to_string(),
            timeout,
        }
    }
}

#[async_trait]
pub trait AiProviderTransport: Send + Sync {
    async fn open(
        &self,
        base_url: &Url,
    ) -> Result<Arc<dyn AiProviderSession>, AiProviderTransportError>;
}

#[async_trait]
pub trait AiProviderSession: Send + Sync {
    async fn request(
        &self,
        method: ProviderMethod,
        endpoint: Url,
        api_key: &str,
        body: Option<Value>,
    ) -> Result<ProviderResponse, AiProviderTransportError>;
}

#[async_trait]
pub trait ProviderDnsResolver: Send + Sync {
    async fn resolve(
        &self,
        host: &str,
        port: u16,
    ) -> Result<Vec<SocketAddr>, AiProviderTransportError>;
}

#[derive(Clone, Copy, Debug, Default)]
struct TokioProviderDnsResolver;

#[async_trait]
impl ProviderDnsResolver for TokioProviderDnsResolver {
    async fn resolve(
        &self,
        host: &str,
        port: u16,
    ) -> Result<Vec<SocketAddr>, AiProviderTransportError> {
        let addresses = tokio::net::lookup_host((host, port))
            .await
            .map_err(|error| AiProviderTransportError::DnsResolutionRejected(error.to_string()))?
            .collect();
        Ok(addresses)
    }
}

pub struct SafeReqwestTransport {
    resolver: Arc<dyn ProviderDnsResolver>,
}

impl SafeReqwestTransport {
    #[must_use]
    pub fn new() -> Self {
        Self {
            resolver: Arc::new(TokioProviderDnsResolver),
        }
    }

    #[must_use]
    pub fn with_resolver(resolver: Arc<dyn ProviderDnsResolver>) -> Self {
        Self { resolver }
    }
}

impl Default for SafeReqwestTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
struct SafeReqwestSession {
    client: reqwest::Client,
    scheme: String,
    host: String,
    port: u16,
}

#[async_trait]
impl AiProviderTransport for SafeReqwestTransport {
    async fn open(
        &self,
        base_url: &Url,
    ) -> Result<Arc<dyn AiProviderSession>, AiProviderTransportError> {
        let (host, port) = validate_origin(base_url)?;
        let addresses = match base_url
            .host()
            .ok_or(AiProviderTransportError::InvalidUrl)?
        {
            Host::Domain(_) => self.resolver.resolve(&host, port).await?,
            Host::Ipv4(address) => vec![SocketAddr::new(IpAddr::V4(address), port)],
            Host::Ipv6(address) => vec![SocketAddr::new(IpAddr::V6(address), port)],
        };
        validate_dns_answers(&addresses)?;
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .resolve_to_addrs(&host, &addresses)
            .build()
            .map_err(|error| AiProviderTransportError::connection_failure(&error, false))?;
        Ok(Arc::new(SafeReqwestSession {
            client,
            scheme: base_url.scheme().to_owned(),
            host,
            port,
        }))
    }
}

#[async_trait]
impl AiProviderSession for SafeReqwestSession {
    async fn request(
        &self,
        method: ProviderMethod,
        endpoint: Url,
        api_key: &str,
        body: Option<Value>,
    ) -> Result<ProviderResponse, AiProviderTransportError> {
        let (host, port) = validate_origin(&endpoint)?;
        if !endpoint.scheme().eq_ignore_ascii_case(&self.scheme)
            || !host.eq_ignore_ascii_case(&self.host)
            || port != self.port
        {
            return Err(AiProviderTransportError::InvalidUrl);
        }
        let builder = match method {
            ProviderMethod::Get => self.client.get(endpoint),
            ProviderMethod::Post => self.client.post(endpoint),
        }
        .bearer_auth(api_key);
        let builder = match body {
            Some(body) => builder.json(&body),
            None => builder,
        };
        let response = builder.send().await.map_err(|error| {
            let timeout = error.is_timeout();
            AiProviderTransportError::connection_failure(&error, timeout)
        })?;
        let status = response.status();
        let body = bounded_json_response(response).await?;
        Ok(ProviderResponse { status, body })
    }
}

fn validate_origin(url: &Url) -> Result<(String, u16), AiProviderTransportError> {
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(AiProviderTransportError::InvalidUrl);
    }
    let host = match url.host().ok_or(AiProviderTransportError::InvalidUrl)? {
        Host::Domain(host) => host.to_owned(),
        Host::Ipv4(host) => host.to_string(),
        Host::Ipv6(host) => host.to_string(),
    };
    let port = url
        .port_or_known_default()
        .ok_or(AiProviderTransportError::InvalidUrl)?;
    Ok((host, port))
}

fn validate_dns_answers(addresses: &[SocketAddr]) -> Result<(), AiProviderTransportError> {
    if addresses.is_empty()
        || addresses
            .iter()
            .any(|address| !is_public_address(address.ip()))
    {
        return Err(AiProviderTransportError::DnsResolutionRejected(
            "all DNS answers must be public addresses".to_owned(),
        ));
    }
    Ok(())
}

pub(crate) fn is_public_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => {
            let octets = address.octets();
            !(octets[0] == 0
                || address.is_private()
                || address.is_loopback()
                || address.is_link_local()
                || address.is_broadcast()
                || address.is_documentation()
                || address.is_unspecified()
                || address.is_multicast()
                || (octets[0] == 100 && (64..=127).contains(&octets[1]))
                || (octets[0] == 192 && octets[1] == 0 && octets[2] == 0)
                || (octets[0] == 198 && matches!(octets[1], 18 | 19))
                || octets[0] >= 240)
        }
        IpAddr::V6(address) => {
            if let Some(mapped) = address.to_ipv4_mapped() {
                return is_public_address(IpAddr::V4(mapped));
            }
            let segments = address.segments();
            let in_2001_special = segments[0] == 0x2001 && segments[1] < 0x0200;
            let in_documentation = address.segments()[0..2] == [0x2001, 0x0db8];
            let in_6to4 = segments[0] == 0x2002;
            let in_reserved = segments[0] == 0x3fff && (segments[1] & 0xf000) == 0;
            (segments[0] & 0xe000) == 0x2000
                && !address.is_unspecified()
                && !address.is_loopback()
                && !address.is_multicast()
                && !in_2001_special
                && !in_documentation
                && !in_6to4
                && !in_reserved
        }
    }
}

async fn bounded_json_response(
    mut response: reqwest::Response,
) -> Result<Value, AiProviderTransportError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_PROVIDER_RESPONSE_BYTES as u64)
    {
        return Err(AiProviderTransportError::ResponseTooLarge);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| {
        let timeout = error.is_timeout();
        AiProviderTransportError::connection_failure(&error, timeout)
    })? {
        if body.len().saturating_add(chunk.len()) > MAX_PROVIDER_RESPONSE_BYTES {
            return Err(AiProviderTransportError::ResponseTooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    decode_bounded_json(&body)
}

fn decode_bounded_json(body: &[u8]) -> Result<Value, AiProviderTransportError> {
    if body.len() > MAX_PROVIDER_RESPONSE_BYTES {
        return Err(AiProviderTransportError::ResponseTooLarge);
    }
    serde_json::from_slice(body)
        .map_err(|error| AiProviderTransportError::InvalidJson(error.to_string()))
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        net::{IpAddr, SocketAddr},
        sync::{Arc, Mutex},
    };

    use async_trait::async_trait;
    use reqwest::Url;

    use super::{
        AiProviderTransport, AiProviderTransportError, MAX_PROVIDER_RESPONSE_BYTES,
        ProviderDnsResolver, ProviderMethod, SafeReqwestTransport, decode_bounded_json,
        is_public_address, validate_dns_answers,
    };

    fn address(value: &str) -> SocketAddr {
        value.parse().unwrap()
    }

    #[test]
    fn address_policy_rejects_loopback_private_reserved_and_special_ranges() {
        for value in [
            "127.0.0.1:443",
            "[::1]:443",
            "169.254.169.254:443",
            "10.0.0.1:443",
            "100.64.0.1:443",
            "172.16.0.1:443",
            "192.168.1.1:443",
            "[fc00::1]:443",
            "[fe80::1]:443",
            "[::]:443",
            "[64:ff9b::1]:443",
            "[ff00::1]:443",
            "0.0.0.0:443",
            "224.0.0.1:443",
            "192.0.0.1:443",
            "192.0.2.1:443",
            "198.18.0.1:443",
            "198.51.100.1:443",
            "203.0.113.1:443",
            "240.0.0.1:443",
            "255.255.255.255:443",
            "[2001::1]:443",
            "[2001:db8::1]:443",
            "[2001:2::1]:443",
            "[2002::1]:443",
            "[3fff::1]:443",
        ] {
            assert!(!is_public_address(address(value).ip()), "{value}");
        }
    }

    #[test]
    fn address_policy_classifies_ipv4_mapped_ipv6_by_mapped_value() {
        assert!(!is_public_address(
            "::ffff:10.0.0.1".parse::<IpAddr>().unwrap()
        ));
        assert!(is_public_address(
            "::ffff:93.184.216.34".parse::<IpAddr>().unwrap()
        ));
    }

    #[test]
    fn address_policy_rejects_empty_or_mixed_answer_sets_and_accepts_public_sets() {
        assert!(matches!(
            validate_dns_answers(&[]),
            Err(AiProviderTransportError::DnsResolutionRejected(_))
        ));
        assert!(matches!(
            validate_dns_answers(&[address("93.184.216.34:443"), address("10.0.0.1:443")]),
            Err(AiProviderTransportError::DnsResolutionRejected(_))
        ));
        assert!(
            validate_dns_answers(&[
                address("93.184.216.34:443"),
                address("1.1.1.1:443"),
                address("[2606:4700:4700::1111]:443")
            ])
            .is_ok()
        );
    }

    #[test]
    fn response_decoder_distinguishes_oversized_and_invalid_json_bodies() {
        assert!(matches!(
            decode_bounded_json(&vec![b' '; MAX_PROVIDER_RESPONSE_BYTES + 1]),
            Err(AiProviderTransportError::ResponseTooLarge)
        ));
        assert!(matches!(
            decode_bounded_json(b"not-json"),
            Err(AiProviderTransportError::InvalidJson(_))
        ));
        assert_eq!(
            decode_bounded_json(br#"{"ok":true}"#).unwrap(),
            serde_json::json!({"ok": true})
        );
    }

    struct ScriptedResolver {
        answers: Mutex<VecDeque<Vec<SocketAddr>>>,
    }

    #[async_trait]
    impl ProviderDnsResolver for ScriptedResolver {
        async fn resolve(
            &self,
            _host: &str,
            _port: u16,
        ) -> Result<Vec<SocketAddr>, AiProviderTransportError> {
            Ok(self.answers.lock().unwrap().pop_front().unwrap())
        }
    }

    #[tokio::test]
    async fn rebinding_second_open_is_rejected_before_connection() {
        let resolver = Arc::new(ScriptedResolver {
            answers: Mutex::new(VecDeque::from([
                vec![address("93.184.216.34:443")],
                vec![address("192.168.1.1:443")],
            ])),
        });
        let transport = SafeReqwestTransport::with_resolver(resolver);
        let origin = Url::parse("https://provider.example.test/v1/").unwrap();
        assert!(transport.open(&origin).await.is_ok());
        assert!(matches!(
            transport.open(&origin).await,
            Err(AiProviderTransportError::DnsResolutionRejected(_))
        ));
    }

    #[tokio::test]
    async fn literal_private_address_is_rejected_without_resolver_fallback() {
        let resolver = Arc::new(ScriptedResolver {
            answers: Mutex::new(VecDeque::new()),
        });
        let transport = SafeReqwestTransport::with_resolver(resolver);
        let origin = Url::parse("https://127.0.0.1/v1/").unwrap();
        assert!(matches!(
            transport.open(&origin).await,
            Err(AiProviderTransportError::DnsResolutionRejected(_))
        ));
    }

    #[tokio::test]
    async fn session_rejects_an_endpoint_from_a_different_origin_before_connection() {
        let resolver = Arc::new(ScriptedResolver {
            answers: Mutex::new(VecDeque::from([vec![address("93.184.216.34:443")]])),
        });
        let transport = SafeReqwestTransport::with_resolver(resolver);
        let origin = Url::parse("https://provider.example.test/v1/").unwrap();
        let session = transport.open(&origin).await.unwrap();
        let endpoint = Url::parse("https://attacker.example.test/v1/models").unwrap();
        assert!(matches!(
            session
                .request(ProviderMethod::Get, endpoint, "secret", None)
                .await,
            Err(AiProviderTransportError::InvalidUrl)
        ));
    }
}
