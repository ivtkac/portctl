use crate::client::{Authenticatable, HttpClient, do_authenticate, handle_response};
use crate::error::Error;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use tracing::info;

#[derive(Deserialize)]
struct AuthResponse {
    token: String,
}

#[derive(Serialize)]
pub struct CreateProxyHostPayload {
    pub domain_names: Vec<String>,
    pub forward_scheme: String,
    pub forward_host: String,
    pub forward_port: u16,
    pub access_list_id: u32,
    pub certificate_id: u32,
    pub ssl_forced: bool,
    pub caching_enabled: bool,
    pub block_exploits: bool,
    pub advanced_config: String,
    pub meta: serde_json::Value,
    pub allow_websocket_upgrade: bool,
    pub http2_support: bool,
    pub hsts_enabled: bool,
    pub hsts_subdomains: bool,
    pub locations: Vec<serde_json::Value>,
}

impl CreateProxyHostPayload {
    pub fn new(
        domain: impl Into<String>,
        scheme: impl Into<String>,
        forward_host: impl Into<String>,
        forward_port: u16,
        websockets: bool,
    ) -> Self {
        Self {
            domain_names: vec![domain.into()],
            forward_scheme: scheme.into(),
            forward_host: forward_host.into(),
            forward_port,
            access_list_id: 0,
            certificate_id: 0,
            ssl_forced: false,
            caching_enabled: false,
            block_exploits: true,
            advanced_config: String::new(),
            meta: serde_json::json!({ "letsencrypt_agree": false, "dns_challenge": false }),
            allow_websocket_upgrade: websockets,
            http2_support: true,
            hsts_enabled: false,
            hsts_subdomains: false,
            locations: vec![],
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct ProxyHost {
    pub id: u32,
    pub domain_names: Vec<String>,
}

pub struct NpmClient {
    http: HttpClient,
    host: String,
}

impl NpmClient {
    pub fn new(base_url: &str, host: &str, secure: bool) -> Self {
        Self {
            http: HttpClient::new(base_url.to_string(), format!("NPM {host}"), secure),
            host: host.to_string(),
        }
    }

    pub async fn list_proxy_hosts(&self) -> Result<Vec<ProxyHost>, Error> {
        self.http.get("/nginx/proxy-hosts").await
    }

    pub async fn create_proxy_host(&self, payload: CreateProxyHostPayload) -> Result<(), Error> {
        let domain = payload.domain_names.first().cloned().unwrap_or_default();
        info!("[NPM:{}] Creating proxy host for '{domain}'...", self.host);

        let res = self.http.post_raw("/nginx/proxy-hosts", &payload).await?;

        handle_response(
            res,
            &format!("create proxy host for '{domain}'"),
            Some(StatusCode::BAD_REQUEST),
            &format!(
                "[NPM:{}] Proxy host for '{domain}' may already exist or data is invalid",
                self.host
            ),
        )
        .await?;

        info!(
            "[NPM:{}] Proxy host for '{domain}' created successfully",
            self.host
        );
        Ok(())
    }

    pub async fn find_host_by_domain(&self, domain: &str) -> Result<Option<ProxyHost>, Error> {
        Ok(self
            .list_proxy_hosts()
            .await?
            .into_iter()
            .find(|h| h.domain_names.iter().any(|d| d == domain)))
    }
}

impl Authenticatable for NpmClient {
    async fn authenticate(&mut self, email: &str, password: &str) -> Result<(), Error> {
        #[derive(Serialize)]
        struct Payload<'a> {
            identity: &'a str,
            secret: &'a str,
        }

        do_authenticate(
            &mut self.http,
            "NPM",
            &self.host.clone(),
            "/tokens",
            &Payload {
                identity: email,
                secret: password,
            },
            |r: AuthResponse| r.token,
        )
        .await
    }
}
