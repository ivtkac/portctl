use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info};

use crate::{
    client::{Authenticatable, EnvVar, HttpClient, do_authenticate, handle_response},
    error::Error,
};

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

#[derive(Deserialize, Debug)]
pub struct PortainerStack {
    #[serde(rename = "Id")]
    pub id: u64,

    #[serde(rename = "Name")]
    pub name: String,

    #[serde(rename = "Status")]
    pub status: Option<u64>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeployStackPayload<'a> {
    name: &'a str,
    stack_file_content: &'a str,
    env: Vec<EnvVar>,
}

pub struct PortainerClient {
    http: HttpClient,
    host: String,
}

impl PortainerClient {
    pub fn new(base_url: &str, host: &str, secure: bool) -> Self {
        Self {
            http: HttpClient::new(base_url.to_string(), format!("Portainer {host}"), secure),
            host: host.to_string(),
        }
    }

    pub async fn get_endpoint_id(&self, endpoint_name: &str) -> Result<u64, Error> {
        debug!("[Portainer:{}] Fetching endpoints...", self.host);
        if endpoint_name == "local" {
            let mut form = HashMap::new();
            form.insert("Name", "local");
            form.insert("EndpointCreationType", "1");
            self.http.post_form("/endpoints", form).await?;
        }
        let endpoints: Vec<Endpoint> = self.http.get("/endpoints").await?;
        endpoints
            .into_iter()
            .find(|e| e.name == endpoint_name)
            .map(|e| e.id)
            .ok_or(Error::EndpointNotFound {
                name: endpoint_name.to_string(),
            })
    }

    pub async fn get_stack_id(&self, endpoint_id: u64, stack_name: &str) -> Result<u64, Error> {
        debug!("[Portainer:{}] Fetching stacks...", self.host);
        let stacks: Vec<PortainerStack> = self
            .http
            .get(&format!("/stacks?endpointId={endpoint_id}"))
            .await?;
        stacks
            .into_iter()
            .find(|s| s.name == stack_name)
            .map(|s| s.id)
            .ok_or(Error::StackNotFound {
                name: stack_name.to_string(),
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
            "[Portainer:{}] Deploying stack '{stack_name}'...",
            self.host
        );
        let endpoint = format!("/stacks/create/standalone/string?endpointId={endpoint_id}");
        let res = self
            .http
            .post_raw(
                &endpoint,
                &DeployStackPayload {
                    name: stack_name,
                    stack_file_content: stack_content,
                    env: env_vars,
                },
            )
            .await?;

        handle_response(
            res,
            &format!("deploy stack '{stack_name}'"),
            Some(StatusCode::CONFLICT),
            &format!(
                "[Portainer:{}] Stack '{stack_name}' already exists — skipping",
                self.host
            ),
        )
        .await?;

        info!(
            "[Portainer:{}] Stack '{stack_name}' deployed successfully",
            self.host
        );
        Ok(())
    }

    pub async fn list_stacks(&self) -> Result<Vec<PortainerStack>, Error> {
        self.http.get("/stacks").await
    }

    pub async fn delete_stack(&self, endpoint_id: u64, stack_id: u64) -> Result<(), Error> {
        self.http
            .delete_raw(&format!("/stacks/{stack_id}?endpointId={endpoint_id}"))
            .await?;
        info!(
            "[Portainer:{}] Stack '{stack_id}' deleted successfully",
            self.host
        );
        Ok(())
    }
}

impl Authenticatable for PortainerClient {
    async fn authenticate(&mut self, username: &str, password: &str) -> Result<(), Error> {
        #[derive(Serialize)]
        struct Payload<'a> {
            username: &'a str,
            password: &'a str,
        }

        do_authenticate(
            &mut self.http,
            "Portainer",
            &self.host.clone(),
            "/auth",
            &Payload { username, password },
            |r: AuthResponse| r.jwt,
        )
        .await
    }
}
