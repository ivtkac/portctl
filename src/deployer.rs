use crate::cli::DeployArgs;
use crate::client::{Authenticatable, CreateProxyHostPayload, NpmClient, PortainerClient};
use crate::credentials::CredentialStore;
use crate::error::Error;
use crate::templates::{ResolvedStack, default_proxies_for_template, resolve_stack};
use std::collections::HashMap;
use tracing::{error, info, warn};

const NPM_TEMPLATES_NAMES: &[&str] = &["npm", "nginx-proxy-manager"];

pub struct Deployer {
    insecure: bool,
}

impl Deployer {
    pub fn new(insecure: bool) -> Self {
        Self { insecure }
    }

    pub async fn run(&self, args: DeployArgs, mut store: CredentialStore) -> Result<(), Error> {
        let (portainer_user, portainer_pass, portainer_url) =
            self.resolve_portainer_creds(&args, &store)?;

        let mut portainer = PortainerClient::new(portainer_url, args.host.clone(), self.insecure);
        portainer
            .authenticate(&portainer_user, &portainer_pass)
            .await?;

        let endpoint_id = portainer.get_endpoint_id(&args.endpoint).await?;
        info!(
            "Using Portainer endpoint '{}' (id={})",
            args.endpoint, endpoint_id
        );

        let overrides: HashMap<String, String> = HashMap::new();
        let mut stacks: Vec<ResolvedStack> = Vec::with_capacity(args.stacks.len());

        for template_name in &args.stacks {
            match resolve_stack(template_name, &args.template_dir, &args.host, &overrides) {
                Ok(stack) => stacks.push(stack),
                Err(e) => error!("Skipping template '{}': {}", template_name, e),
            }
        }

        if stacks.is_empty() {
            return Err(Error::other("No valid stack templates resolved — aborting"));
        }

        let stack_results = self.deploy_stacks(&portainer, endpoint_id, &stacks).await;
        self.persist_generated_creds(&stacks, &stack_results, &args.host, &mut store)?;

        if args.enable_proxy {
            let sucessfully_deployed: Vec<&ResolvedStack> = stacks
                .iter()
                .filter(|s| *stack_results.get(&s.name).unwrap_or(&false))
                .collect();

            self.deploy_proxies(&args, &store, &sucessfully_deployed)
                .await;
        }

        self.print_summary(&stack_results);

        if stack_results.values().all(|&ok| ok) {
            Ok(())
        } else {
            Err(Error::other("One or more deployments failed"))
        }
    }

    fn resolve_portainer_creds(
        &self,
        args: &DeployArgs,
        store: &CredentialStore,
    ) -> Result<(String, String, String), Error> {
        let stored = store.get(&args.host, "portainer");

        let user = args
            .user
            .as_deref()
            .or_else(|| stored.and_then(|c| c.user.as_deref()))
            .ok_or_else(|| {
                Error::other(format!(
                    "No Portainer credentials found for {}.\nRun: portctl creds set -H {} -s portainer -u <user> -p <password>",
                    args.host, args.host
                ))
            })?
            .to_string();

        let password = args
            .password
            .as_deref()
            .or_else(|| stored.and_then(|c| c.password.as_deref()))
            .ok_or_else(|| {
                Error::other(format!(
                    "No Portainer credentials found for {}. \
                     Run: portctl creds set -H {} -s portainer -u <user> -p <password>",
                    args.host, args.host
                ))
            })?
            .to_string();

        let url = stored
            .and_then(|c| c.url.as_deref())
            .map(String::from)
            .unwrap_or_else(|| args.portainer_base_url());

        Ok((user, password, url))
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
            let deployed_ok = *results.get(&stack.name).unwrap_or(&false);
            if !deployed_ok {
                continue;
            }

            let creds = &stack.generated_credentials;
            if creds.user.is_none() && creds.password.is_none() {
                continue;
            }

            let service = if NPM_TEMPLATES_NAMES.contains(&stack.template_name.as_str()) {
                "npm"
            } else {
                &stack.template_name
            };

            info!(
                "Auto-saved generated credentials for {} stack '{}' → stored as [{service}@{host}]",
                stack.template_name, stack.name
            );
            any_saved = true;
        }

        if any_saved {
            store.save()?;
        }

        Ok(())
    }

    async fn deploy_proxies(
        &self,
        args: &DeployArgs,
        store: &CredentialStore,
        stacks: &[&ResolvedStack],
    ) {
        let Some(npm_creds) = store.get(args.npm_host(), "npm") else {
            error!(
                "No NPM credentials found for {} — was the npm stack deployed?\nRe-run without --enable-proxy first, then retry.",
                args.npm_host()
            );
            return;
        };

        let (Some(npm_user), Some(npm_pass)) = (&npm_creds.user, &npm_creds.password) else {
            error!("NPM credentials for {} are incomplete", args.npm_host());
            return;
        };

        let npm_url = npm_creds
            .url
            .as_deref()
            .map(String::from)
            .unwrap_or_else(|| args.npm_base_url());

        let mut npm = NpmClient::new(npm_url, args.npm_host().to_string(), self.insecure);

        if let Err(e) = npm.authenticate(npm_user, npm_pass).await {
            error!("NPM authentication failed — skipping proxy setup: {e}");
            return;
        }

        for stack in stacks {
            let proxies = default_proxies_for_template(&stack.template_name, &args.host);

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
    }

    fn print_summary(&self, results: &HashMap<String, bool>) {
        let total = results.len();
        let ok = results.values().filter(|&&v| v).count();

        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        info!("DEPLOYMENT SUMMARY  — {ok}/{total} succeeded");
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

        let mut names: Vec<_> = results.iter().collect();
        names.sort_by_key(|(name, _)| *name);

        for (name, &success) in names {
            let status = if success { "OK" } else { "ERROR" };
            info!("  {status} {name}")
        }
    }
}
