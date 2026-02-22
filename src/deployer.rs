use crate::cli::{ProxyEnableArgs, ProxyListArgs, StackDeployArgs, StackListArgs, StackRemoveArgs};
use crate::client::{Authenticatable, CreateProxyHostPayload, NpmClient, PortainerClient};
use crate::credentials::{CredentialStore, Credentials};
use crate::error::Error;
use crate::templates::{ResolvedStack, default_proxies_for_template, resolve_stack};
use std::collections::HashMap;
use tracing::{error, info, warn};

const NPM_TEMPLATE_NAMES: &[&str] = &["npm", "nginx-proxy-manager"];

trait PortainerArgs {
    fn host(&self) -> &str;
    fn user(&self) -> Option<&str>;
    fn password(&self) -> Option<&str>;
    fn portainer_base_url(&self) -> String;
}

trait NpmArgs {
    fn npm_host(&self) -> &str;
    fn npm_base_url(&self) -> String;
}

impl PortainerArgs for StackDeployArgs {
    fn host(&self) -> &str {
        &self.host
    }
    fn user(&self) -> Option<&str> {
        self.user.as_deref()
    }
    fn password(&self) -> Option<&str> {
        self.password.as_deref()
    }
    fn portainer_base_url(&self) -> String {
        StackDeployArgs::portainer_base_url(self)
    }
}

impl PortainerArgs for StackListArgs {
    fn host(&self) -> &str {
        &self.host
    }
    fn user(&self) -> Option<&str> {
        self.user.as_deref()
    }
    fn password(&self) -> Option<&str> {
        self.password.as_deref()
    }
    fn portainer_base_url(&self) -> String {
        StackListArgs::portainer_base_url(self)
    }
}

impl PortainerArgs for StackRemoveArgs {
    fn host(&self) -> &str {
        &self.host
    }
    fn user(&self) -> Option<&str> {
        self.user.as_deref()
    }
    fn password(&self) -> Option<&str> {
        self.password.as_deref()
    }
    fn portainer_base_url(&self) -> String {
        StackRemoveArgs::portainer_base_url(self)
    }
}

impl NpmArgs for ProxyEnableArgs {
    fn npm_host(&self) -> &str {
        ProxyEnableArgs::npm_host(self)
    }
    fn npm_base_url(&self) -> String {
        ProxyEnableArgs::npm_base_url(self)
    }
}

impl NpmArgs for ProxyListArgs {
    fn npm_host(&self) -> &str {
        ProxyListArgs::npm_host(self)
    }
    fn npm_base_url(&self) -> String {
        ProxyListArgs::npm_base_url(self)
    }
}

pub struct Deployer {
    secure: bool,
}

impl Deployer {
    pub fn new(secure: bool) -> Self {
        Self { secure }
    }

    pub async fn stack_deploy(
        &self,
        args: StackDeployArgs,
        mut store: CredentialStore,
    ) -> Result<(), Error> {
        let portainer = self.build_portainer_client(&args, &store).await?;
        let endpoint_id = portainer.get_endpoint_id(&args.endpoint).await?;
        info!(
            "Using Portainer endpoint '{}' (id={})",
            args.endpoint, endpoint_id
        );

        let overrides = HashMap::new();
        let stacks: Vec<ResolvedStack> = args
            .name
            .iter()
            .filter_map(|template_name| {
                let service = npm_service_key(template_name);
                let existing_creds = store.get(&args.host, service);
                resolve_stack(
                    template_name,
                    &args.template_dir,
                    &args.host,
                    &overrides,
                    existing_creds,
                )
                .map_err(|e| error!("Skipping template '{template_name}': {e}"))
                .ok()
            })
            .collect();

        if stacks.is_empty() {
            return Err(Error::Other(format!(
                "No valid stack templates resolved — aborting"
            )));
        }

        let results = self.deploy_stacks(&portainer, endpoint_id, &stacks).await;
        self.persist_generated_creds(&stacks, &results, &args.host, &mut store)?;
        self.print_summary(&results);

        if results.values().all(|&ok| ok) {
            Ok(())
        } else {
            Err(Error::Other(format!("One or more deployments failed")))
        }
    }

    pub async fn stack_list(
        &self,
        args: StackListArgs,
        store: CredentialStore,
    ) -> Result<(), Error> {
        let portainer = self.build_portainer_client(&args, &store).await?;
        let stacks = portainer.list_stacks().await?;

        if stacks.is_empty() {
            println!("No stacks found on {}", args.host);
            return Ok(());
        }

        println!("{:<4}  {:<30}  {}", "ID", "NAME", "STATUS");
        println!("{}", "─".repeat(55));
        for s in &stacks {
            println!("{:<4}  {:<30}  {}", s.id, s.name, "unimplemented");
        }
        Ok(())
    }

    pub async fn stack_remove(
        &self,
        args: StackRemoveArgs,
        store: CredentialStore,
    ) -> Result<(), Error> {
        let portainer = self.build_portainer_client(&args, &store).await?;
        let endpoint_id = portainer.get_endpoint_id(&args.endpoint).await?;
        let stack_id = portainer.get_stack_id(endpoint_id, &args.name).await?;
        portainer.delete_stack(endpoint_id, stack_id).await?;
        Ok(())
    }

    pub async fn proxy_enable(
        &self,
        args: ProxyEnableArgs,
        store: CredentialStore,
    ) -> Result<(), Error> {
        let npm = self.build_npm_client(&args, &store).await?;

        for template_name in &args.stack {
            let proxies = default_proxies_for_template(template_name, &args.host);
            if proxies.is_empty() {
                warn!("No proxy definitions found for template '{template_name}' — skipping");
                continue;
            }
            for proxy in proxies {
                match npm.find_host_by_domain(&proxy.domain).await {
                    Ok(Some(_)) => {
                        warn!(
                            "Proxy host for '{}' already exists — skipping",
                            proxy.domain
                        );
                        continue;
                    }
                    Err(e) => {
                        error!("Could not check existing proxy hosts: {e}");
                        continue;
                    }
                    Ok(None) => {}
                }
                let payload = CreateProxyHostPayload::new(
                    &proxy.domain,
                    &proxy.scheme,
                    &proxy.forward_host,
                    proxy.forward_port,
                    proxy.websockets,
                );
                if let Err(e) = npm.create_proxy_host(payload).await {
                    error!("Failed to create proxy host for '{}': {e}", proxy.domain);
                }
            }
        }
        Ok(())
    }

    pub async fn proxy_list(
        &self,
        args: ProxyListArgs,
        store: CredentialStore,
    ) -> Result<(), Error> {
        let npm = self.build_npm_client(&args, &store).await?;
        let hosts = npm.list_proxy_hosts().await?;

        if hosts.is_empty() {
            println!("No proxy hosts found on {}", args.npm_host());
            return Ok(());
        }

        println!("{:<6}  {}", "ID", "DOMAINS");
        println!("{}", "─".repeat(55));
        for h in &hosts {
            println!("{:<6}  {}", h.id, h.domain_names.join(", "));
        }
        Ok(())
    }

    async fn build_portainer_client(
        &self,
        args: &impl PortainerArgs,
        store: &CredentialStore,
    ) -> Result<PortainerClient, Error> {
        let host = args.host();
        let stored = store.get(host, "portainer");

        let user = args
            .user()
            .or_else(|| stored.and_then(|c| c.user.as_deref()))
            .ok_or_else(|| missing_portainer_creds(host))?
            .to_string();

        let password = args
            .password()
            .or_else(|| stored.and_then(|c| c.password.as_deref()))
            .ok_or_else(|| missing_portainer_creds(host))?
            .to_string();

        let url = stored
            .and_then(|c| c.url.as_deref())
            .map(String::from)
            .unwrap_or_else(|| args.portainer_base_url());

        let mut client = PortainerClient::new(url.as_str(), host, self.secure);
        client.authenticate(&user, &password).await?;
        Ok(client)
    }

    async fn build_npm_client(
        &self,
        args: &impl NpmArgs,
        store: &CredentialStore,
    ) -> Result<NpmClient, Error> {
        let npm_host = args.npm_host();
        let npm_creds = store.get(npm_host, "npm").ok_or_else(|| {
            Error::Other(format!(
                "No NPM credentials found for {npm_host}.\n\
                     Deploy the npm stack first, or set credentials with:\n\
                     portctl creds set -H {npm_host} -s npm -u <user> -p <pass>"
            ))
        })?;

        let (user, pass) = extract_creds(npm_creds, "NPM", npm_host)?;
        let url = npm_creds
            .url
            .as_deref()
            .map(String::from)
            .unwrap_or_else(|| args.npm_base_url());

        let mut client = NpmClient::new(url.as_str(), npm_host, self.secure);
        client.authenticate(&user, &pass).await?;
        Ok(client)
    }

    async fn deploy_stacks(
        &self,
        portainer: &PortainerClient,
        endpoint_id: u64,
        stacks: &[ResolvedStack],
    ) -> HashMap<String, bool> {
        let mut results = HashMap::new();
        for stack in stacks {
            let ok = portainer
                .deploy_stack(
                    &stack.name,
                    &stack.compose_content,
                    endpoint_id,
                    stack.env_vars.clone(),
                )
                .await
                .map(|_| true)
                .unwrap_or_else(|e| {
                    error!("Failed to deploy stack '{}': {}", stack.name, e);
                    false
                });
            results.insert(stack.name.clone(), ok);
        }
        results
    }

    fn persist_generated_creds(
        &self,
        stacks: &[ResolvedStack],
        results: &HashMap<String, bool>,
        host: &str,
        store: &mut CredentialStore,
    ) -> Result<(), Error> {
        let mut any_saved = false;
        for stack in stacks {
            if !results.get(&stack.name).copied().unwrap_or(false) {
                continue;
            }
            let creds = &stack.generated_credentials;
            if creds.user.is_none() && creds.password.is_none() {
                continue;
            }

            let service = npm_service_key(&stack.template_name);
            store.patch(
                host,
                service,
                Credentials {
                    user: creds.user.clone(),
                    password: creds.password.clone(),
                    url: None,
                },
            );
            info!(
                "Auto-saved generated credentials for '{}' → [{service}@{host}]",
                stack.name
            );
            any_saved = true;
        }
        if any_saved {
            store.save()?;
        }
        Ok(())
    }

    fn print_summary(&self, results: &HashMap<String, bool>) {
        let ok = results.values().filter(|&&v| v).count();
        let total = results.len();
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        info!("DEPLOYMENT SUMMARY — {ok}/{total} succeeded");
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        let mut names: Vec<_> = results.iter().collect();
        names.sort_by_key(|(name, _)| *name);
        for (name, &success) in names {
            info!("  {}  {name}", if success { "✓ OK   " } else { "✗ ERROR" });
        }
    }
}

fn npm_service_key(template_name: &str) -> &str {
    if NPM_TEMPLATE_NAMES.contains(&template_name) {
        "npm"
    } else {
        template_name
    }
}

fn missing_portainer_creds(host: &str) -> Error {
    Error::Other(format!(
        "No Portainer credentials found for {host}.\n\
         Run: portctl creds set -H {host} -s portainer -u <user> -p <pass>"
    ))
}

fn extract_creds(
    creds: &Credentials,
    service: &str,
    host: &str,
) -> Result<(String, String), Error> {
    let user = creds.user.clone().ok_or_else(|| {
        Error::Other(format!(
            "{service} credentials for {host} are missing a username"
        ))
    })?;
    let pass = creds.password.clone().ok_or_else(|| {
        Error::Other(format!(
            "{service} credentials for {host} are missing a password"
        ))
    })?;
    Ok((user, pass))
}
