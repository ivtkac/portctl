use crate::error::Error;
use reqwest::{Client, Response, StatusCode, multipart};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, time::Duration};
use tracing::{debug, info, warn};

pub mod npm;
pub mod portainer;

pub use npm::*;
pub use portainer::*;

pub struct HttpClient {
    inner: Client,
    pub base_url: String,
    jwt_token: Option<String>,
    tag: String,
}

impl HttpClient {
    pub fn new(base_url: String, tag: impl Into<String>, secure: bool) -> Self {
        let inner = Client::builder()
            .danger_accept_invalid_certs(!secure)
            .timeout(Duration::from_secs(60))
            .build()
            .expect("failed to build HTTP client");

        Self {
            inner,
            base_url: base_url.trim_end_matches('/').to_string(),
            jwt_token: None,
            tag: tag.into(),
        }
    }

    pub fn set_token(&mut self, token: String) {
        self.jwt_token = Some(token);
    }

    pub fn url(&self, endpoint: &str) -> String {
        format!("{}/api/{}", self.base_url, endpoint.trim_start_matches('/'))
    }

    fn auth_header(&self) -> Option<String> {
        self.jwt_token.as_deref().map(|t| format!("Bearer {t}"))
    }

    fn authorize(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match self.auth_header() {
            Some(auth) => builder.header("Authorization", auth),
            None => builder,
        }
    }

    pub async fn get<R>(&self, endpoint: &str) -> Result<R, Error>
    where
        R: for<'de> Deserialize<'de>,
    {
        debug!("[{}] GET {}", self.tag, endpoint);
        let res = self
            .authorize(self.inner.get(self.url(endpoint)))
            .send()
            .await?
            .error_for_status()?;
        Ok(res.json().await?)
    }

    pub async fn post<B, R>(&self, endpoint: &str, body: &B) -> Result<R, Error>
    where
        B: Serialize,
        R: for<'de> Deserialize<'de>,
    {
        debug!("[{}] POST {}", self.tag, endpoint);
        let res = self
            .authorize(self.inner.post(self.url(endpoint)).json(body))
            .send()
            .await?;
        debug!("[{}] POST {} -> {}", self.tag, endpoint, res.status());
        Ok(res.error_for_status()?.json().await?)
    }

    pub async fn post_raw<B>(&self, endpoint: &str, body: &B) -> Result<Response, Error>
    where
        B: Serialize,
    {
        debug!("[{}] POST {}", self.tag, endpoint);
        let res = self
            .authorize(self.inner.post(self.url(endpoint)).json(body))
            .send()
            .await?;
        debug!("[{}] POST {} -> {}", self.tag, endpoint, res.status());
        Ok(res)
    }

    pub async fn post_form(
        &self,
        endpoint: &str,
        values: HashMap<&'static str, &'static str>,
    ) -> Result<Response, Error> {
        debug!("[{}] POST {}", self.tag, endpoint);
        let mut form = multipart::Form::new();
        for (k, v) in values {
            form = form.text(k, v);
        }
        let res = self
            .authorize(self.inner.post(self.url(endpoint)).multipart(form))
            .send()
            .await?;
        Ok(res)
    }

    pub async fn delete_raw(&self, endpoint: &str) -> Result<Response, Error> {
        debug!("[{}] DELETE {}", self.tag, endpoint);
        let res = self
            .authorize(self.inner.delete(self.url(endpoint)))
            .send()
            .await?;
        debug!("[{}] DELETE {} -> {}", self.tag, endpoint, res.status());
        Ok(res)
    }
}

pub trait Authenticatable {
    fn authenticate(
        &mut self,
        username: &str,
        password: &str,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send;
}

async fn do_authenticate<'a, P, R>(
    http: &'a mut HttpClient,
    service: &str,
    host: &str,
    endpoint: &str,
    payload: &P,
    extract_token: impl Fn(R) -> String,
) -> Result<(), Error>
where
    P: Serialize,
    R: for<'de> Deserialize<'de>,
{
    info!("[{service}:{host}] Authenticating...");
    let res: R = http
        .post(endpoint, payload)
        .await
        .map_err(|_| Error::AuthFailed {
            service: service.into(),
            host: host.into(),
        })?;
    http.set_token(extract_token(res));
    info!("[{service}:{host}] Authentication successful");
    Ok(())
}

async fn handle_response(
    res: Response,
    operation: &str,
    conflict_status: Option<StatusCode>,
    conflict_msg: &str,
) -> Result<(), Error> {
    let status = res.status();
    if status.is_success() {
        return Ok(());
    }

    if conflict_status.map_or(false, |s| s == status) {
        warn!("{conflict_msg}");
        return Ok(());
    }

    let body = res.text().await.unwrap_or_default();
    Err(Error::ApiError {
        status: status.as_u16(),
        operation: operation.into(),
        body,
    })
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EnvVar {
    pub name: String,
    pub value: String,
}
