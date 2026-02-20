use anyhow::Result;
use clap::Parser;
use portctl::{
    cli::{Cli, Commands, CredsCommands},
    credentials::{CredentialStore, Credentials},
    deployer::Deployer,
    templates,
};
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    init_logging(cli.verbose);

    let creds_path = cli.resolved_credentials_path();

    match cli.command {
        Commands::Deploy(args) => {
            let store = CredentialStore::load(&creds_path)?;
            let deployer = Deployer::new(args.insecure);
            deployer.run(args, store).await?;
        }
        Commands::Creds(sub) => {
            let mut store = CredentialStore::load(&creds_path)?;

            match sub {
                CredsCommands::Set(args) => {
                    if args.service != "portainer" {
                        eprintln!(
                            "Only 'portainer' credentials are manually managed.\n\
                             NPM credentials are auto-generated and saved when you deploy the npm stack."
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

                CredsCommands::List => {
                    let entries = store.all_entries();
                    if entries.is_empty() {
                        println!("No credentials stored in {}", creds_path.display());
                    } else {
                        println!("Stored credentials ({}):\n", creds_path.display());
                        for (host, service, creds) in entries {
                            let user = creds.user.as_deref().unwrap_or("<not set>");
                            let pass = creds
                                .password
                                .as_ref()
                                .map(|p| "*".repeat(p.len().min(12)))
                                .unwrap_or_else(|| "<not set>".to_string());
                            let url = creds.url.as_deref().unwrap_or("-");
                            println!("  [{service}@{host}]");
                            println!("    user     = {user}");
                            println!("    password = {pass}");
                            println!("    url      = {url}");
                        }
                    }
                }

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
        Commands::ListTemplates => {
            templates::print_available_templates();
        }
    }

    Ok(())
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
