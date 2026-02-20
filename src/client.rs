use std::time::Duration;

use reqwest::{Client, Response, StatusCode, multipart};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::error::Error;

pub struct HttpClient {
    inner: Client,
    pub base_url: String,
    jwt_token: Option<String>,
    service_tag: String,
}

impl HttpClient {
    pub fn new(base_url: String, service_tag: impl Into<String>, insecure: bool) -> Self {
        let inner = Client::builder()
            .danger_accept_invalid_certs(insecure)
            .timeout(Duration::from_secs(60))
            .build()
            .expect("failed to build HTTP client");

        Self {
            inner,
            base_url: base_url.trim_end_matches('/').to_string(),
            jwt_token: None,
            service_tag: service_tag.into(),
        }
    }

    pub fn set_token(&mut self, token: String) {
        self.jwt_token = Some(token)
    }

    fn tag(&self) -> &str {
        &self.service_tag
    }

    pub fn url(&self, endpoint: &str) -> String {
        format!("{}/api/{}", self.base_url, endpoint.trim_start_matches('/'))
    }

    fn auth_header(&self) -> Option<String> {
        self.jwt_token.as_deref().map(|t| format!("Bearer {t}"))
    }

    pub async fn post<B, R>(&self, endpoint: &str, body: &B) -> Result<R, Error>
    where
        B: Serialize,
        R: for<'de> Deserialize<'de>,
    {
        let url = self.url(endpoint);
        debug!("[{}] POST {}", self.tag(), endpoint);

        let mut req = self.inner.post(&url).json(body);
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }

        let res = req.send().await?;
        debug!("[{}] POST {} -> {}", self.tag(), endpoint, res.status());

        let res = res.error_for_status()?;
        Ok(res.json().await?)
    }

    pub async fn post_raw<B>(&self, endpoint: &str, body: &B) -> Result<Response, Error>
    where
        B: Serialize,
    {
        let url = self.url(endpoint);
        debug!("[{}] POST {}", self.tag(), endpoint);

        let mut req = self.inner.post(&url).json(body);
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }

        Ok(req.send().await?)
    }

    pub async fn post_form(
        &self,
        endpoint: &str,
        form: multipart::Form,
    ) -> Result<Response, Error> {
        let url = self.url(endpoint);
        debug!("[{}] POST {}", self.tag(), endpoint);

        let mut req = self.inner.post(&url).multipart(form);
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }

        Ok(req.send().await?)
    }

    pub async fn get<R>(&self, endpoint: &str) -> Result<R, Error>
    where
        R: for<'de> Deserialize<'de>,
    {
        let url = self.url(endpoint);
        debug!("[{}] GET {}", self.tag(), endpoint);

        let mut req = self.inner.get(&url);
        if let Some(auth) = self.auth_header() {
            req = req.header("Authorization", auth);
        }

        let res = req.send().await?.error_for_status()?;
        Ok(res.json().await?)
    }
}

pub trait Authenticatable {
    fn authenticate(
        &mut self,
        username: &str,
        password: &str,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send;
}

#[derive(Serialize)]
struct AuthPayload<'a> {
    username: &'a str,
    password: &'a str,
}

#[derive(Deserialize)]
struct AuthResponse {
    jwt: String,
}

#[derive(Deserialize)]
pub struct Endpoint {
    #[serde(rename = "Id")]
    pub id: u64,
    #[serde(rename = "Name")]
    pub name: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeployStackPayload<'a> {
    name: &'a str,
    stack_file_content: &'a str,
    env: Vec<EnvVar>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EnvVar {
    pub name: String,
    pub value: String,
}

pub struct PortainerClient {
    http: HttpClient,
    host: String,
}

impl PortainerClient {
    pub fn new(base_url: String, host: String, insecure: bool) -> Self {
        let http = HttpClient::new(base_url, format!("Portainer {host}"), insecure);
        Self { http, host }
    }

    pub async fn get_endpoint_id(&self, endpoint_name: &str) -> Result<u64, Error> {
        debug!("[Portainer:{}] Fetching endpoints...", self.host);

        if endpoint_name == "local" {
            let form = multipart::Form::new()
                .text("Name", "local")
                .text("EndpointCreationType", "1");
            self.http.post_form("/endpoints", form).await?;
        }

        let endpoints: Vec<Endpoint> = self.http.get("/endpoints").await?;
        endpoints
            .into_iter()
            .find(|e| e.name == endpoint_name)
            .map(|e| e.id)
            .ok_or_else(|| Error::EndpointNotFound {
                name: endpoint_name.to_string(),
            })
    }

    pub async fn deploy_stack(
        &self,
        stack_name: &str,
        stack_content: &str,
        endpoint_id: u64,
        env_vars: Vec<EnvVar>,
    ) -> Result<(), Error> {
        info!(
            "[Portainer:{}] Deploying stack '{}'...",
            self.host, stack_name
        );

        let payload = DeployStackPayload {
            name: stack_name,
            stack_file_content: stack_content,
            env: env_vars,
        };

        let endpoint = format!("/stacks/create/standalone/string?endpointId={endpoint_id}");
        let res = self.http.post_raw(&endpoint, &payload).await?;

        match res.status() {
            s if s.is_success() => {
                info!(
                    "[Portainer:{}] Stack '{}' deployed successfully",
                    self.host, stack_name
                );
                Ok(())
            }
            StatusCode::CONFLICT => {
                warn!(
                    "[Portainer:{}] Stack '{}' already exists — skipping",
                    self.host, stack_name
                );
                Ok(())
            }
            s => {
                let body = res.text().await.unwrap_or_default();
                Err(Error::ApiError {
                    status: s.as_u16(),
                    operation: format!("deploy stack '{stack_name}'"),
                    body,
                })
            }
        }
    }
}

impl Authenticatable for PortainerClient {
    async fn authenticate(&mut self, username: &str, password: &str) -> Result<(), Error> {
        info!("[Portainer:{}] Authenticating...", self.host);

        let res: AuthResponse = self
            .http
            .post("/auth", &AuthPayload { username, password })
            .await
            .map_err(|_| Error::AuthFailed {
                service: "Portainer".into(),
                host: self.host.clone(),
            })?;

        self.http.set_token(res.jwt);
        info!("[Portainer:{}] Authentication successful", self.host);
        Ok(())
    }
}

#[derive(Serialize)]
struct NpmAuthPayload<'a> {
    identity: &'a str,
    secret: &'a str,
}

#[derive(Deserialize)]
struct NpmAuthResponse {
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
    pub fn new(base_url: String, host: String, insecure: bool) -> Self {
        let http = HttpClient::new(base_url, format!("NPM {host}"), insecure);
        Self { http, host }
    }

    pub async fn list_proxy_hosts(&self) -> Result<Vec<ProxyHost>, Error> {
        self.http.get("/nginx/proxy-hosts").await
    }

    pub async fn create_proxy_host(&self, payload: CreateProxyHostPayload) -> Result<(), Error> {
        let domain = payload.domain_names.first().cloned().unwrap_or_default();
        info!(
            "[NPM:{}] Creating proxy host for '{}'...",
            self.host, domain
        );

        let res = self.http.post_raw("/nginx/proxy-hosts", &payload).await?;
        match res.status() {
            s if s.is_success() => {
                info!(
                    "[NPM:{}] Proxy host for '{}' created successfully",
                    self.host, domain
                );
                Ok(())
            }
            StatusCode::BAD_REQUEST => {
                let body = res.text().await.unwrap_or_default();
                warn!(
                    "[NPM:{}] Proxy host for '{}' may already exist or data is invalid: {}",
                    self.host, domain, body
                );
                Ok(())
            }
            s => {
                let body = res.text().await.unwrap_or_default();
                Err(Error::ApiError {
                    status: s.as_u16(),
                    operation: format!("create proxy host '{domain}'"),
                    body,
                })
            }
        }
    }

    pub async fn find_host_by_domain(&self, domain: &str) -> Result<Option<ProxyHost>, Error> {
        let hosts = self.list_proxy_hosts().await?;
        Ok(hosts
            .into_iter()
            .find(|h| h.domain_names.iter().any(|d| d == domain)))
    }
}

impl Authenticatable for NpmClient {
    async fn authenticate(&mut self, email: &str, password: &str) -> Result<(), Error> {
        info!("[NPM:{}] Authenticating...", self.host);

        let res: NpmAuthResponse = self
            .http
            .post(
                "/tokens",
                &NpmAuthPayload {
                    identity: email,
                    secret: password,
                },
            )
            .await
            .map_err(|_| Error::AuthFailed {
                service: "NPM".into(),
                host: self.host.clone(),
            })?;

        self.http.set_token(res.token);
        info!("[NPM:{}] Authentication successful", self.host);
        Ok(())
    }
}
