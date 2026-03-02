pub mod cli;
pub mod client;
pub mod credentials;
pub mod deployer;
pub mod error;
pub mod templates;
pub mod utils;

use crate::{
    cli::{Cli, Commands, CredsCommands, ProxyCommands, StackCommands, TemplatesCommands},
    credentials::{CredentialStore, Credentials},
    deployer::Deployer,
};

use anyhow::Result;
use clap::Parser;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_logging(cli.verbose);

    let creds_path = cli.resolved_credentials_file();
    let load_store = || CredentialStore::load(&creds_path);

    match cli.command {
        Commands::Stack(sub) => match sub {
            StackCommands::Deploy(args) => {
                Deployer::new(args.portainer.secure)
                    .stack_deploy(args, load_store()?)
                    .await?;
            }
            StackCommands::Remove(args) => {
                Deployer::new(args.portainer.secure)
                    .stack_remove(args, load_store()?)
                    .await?;
            }
            StackCommands::List(args) => {
                Deployer::new(args.portainer.secure)
                    .stack_list(args, load_store()?)
                    .await?;
            }
        },

        Commands::Proxy(sub) => match sub {
            ProxyCommands::Enable(args) => {
                Deployer::new(args.npm.secure)
                    .proxy_enable(args, load_store()?)
                    .await?;
            }
            ProxyCommands::List(args) => {
                Deployer::new(args.npm.secure)
                    .proxy_list(args, load_store()?)
                    .await?;
            }
        },

        Commands::Creds(sub) => {
            let mut store = load_store()?;
            match sub {
                CredsCommands::Set(args) => {
                    if args.service != "portainer" {
                        eprintln!(
                            "Only 'portainer' credentials are manually managed.\n\
                                 NPM credentials are auto-generated and saved when you \
                                 deploy the npm stack."
                        );
                        return Ok(());
                    }
                    store.patch(
                        &args.host,
                        &args.service,
                        Credentials {
                            user: args.user,
                            password: args.password,
                            url: args.url,
                        },
                    );
                    store.save()?;
                    info!(
                        "Credentials for {}@{} saved to {}",
                        args.service,
                        args.host,
                        creds_path.display()
                    );
                }
                CredsCommands::List => print_creds(&store, &creds_path.display().to_string()),
                CredsCommands::Remove(args) => {
                    if store.remove(&args.host, &args.service) {
                        store.save()?;
                        info!("Credentials for {}@{} removed", args.service, args.host);
                    } else {
                        eprintln!("No credentials found for {}@{}", args.service, args.host);
                    }
                }
            }
        }

        Commands::Templates(sub) => match sub {
            // TemplatesCommands::List => templates::print_available_templates(),
            TemplatesCommands::List(args) => {
                Deployer::new(args.portainer.secure)
                    .list_templates(args, load_store()?)
                    .await?;
            }
            TemplatesCommands::Show(args) => {
                Deployer::new(args.portainer.secure)
                    .show_template(args, load_store()?)
                    .await?;
            }
        },
    }

    Ok(())
}

fn print_creds(store: &CredentialStore, path: &str) {
    let entries = store.all_entries();
    if entries.is_empty() {
        println!("No credentials stored in {path}");
        return;
    }
    println!("Stored credentials ({path}):\n");
    for (host, service, creds) in entries {
        let user = creds.user.as_deref().unwrap_or("");
        let pass = creds
            .password
            .as_ref()
            .map(|p| "*".repeat(p.len().min(12)))
            .unwrap_or_default();
        let url = creds.url.as_deref().unwrap_or("-");
        println!("  [{service}@{host}]");
        println!("    user     = {user}");
        println!("    password = {pass}");
        println!("    url      = {url}");
    }
}

fn init_logging(verbose: bool) {
    let level = if verbose { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(level)),
        )
        .with_target(false)
        .init();
}
