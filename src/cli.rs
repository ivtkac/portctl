use crate::credentials::default_credentials_path;
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "portctl",
    version,
    about,
    long_about = "Deploy Docker stacks to Portainer and optionally configure Nginx Proxy Manager proxy hosts."
)]
pub struct Cli {
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[arg(long, global = true)]
    pub credentials_file: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

impl Cli {
    pub fn resolved_credentials_file(&self) -> PathBuf {
        self.credentials_file
            .clone()
            .unwrap_or_else(default_credentials_path)
    }
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    #[command(subcommand)]
    Stack(StackCommands),

    #[command(subcommand)]
    Proxy(ProxyCommands),

    #[command(subcommand)]
    Creds(CredsCommands),

    #[command(subcommand)]
    Templates(TemplatesCommands),
}

#[derive(Subcommand, Debug)]
pub enum StackCommands {
    Deploy(StackDeployArgs),
    Remove(StackRemoveArgs),
    List(StackListArgs),
}

#[derive(Args, Debug, Clone)]
pub struct PortainerConnectionArgs {
    #[arg(long, short = 'H', env = "PORTAINER_HOST")]
    pub host: String,

    #[arg(long, short = 'u', env = "PORTAINER_USER")]
    pub user: Option<String>,

    #[arg(long, short = 'p', env = "PORTAINER_PASSWORD")]
    pub password: Option<String>,

    #[arg(long, default_value = "9443")]
    pub portainer_port: u16,

    #[arg(long)]
    pub portainer_url: Option<String>,

    #[arg(long)]
    pub secure: bool,
}

impl PortainerConnectionArgs {
    pub fn base_url(&self) -> String {
        self.portainer_url
            .clone()
            .unwrap_or_else(|| format!("https://{}:{}", self.host, self.portainer_port))
    }
}

#[derive(Args, Debug, Clone)]
pub struct NpmConnectionArgs {
    #[arg(long, short = 'H', env = "PORTAINER_HOST")]
    pub host: String,

    #[arg(long)]
    pub npm_host: Option<String>,

    #[arg(long, default_value = "81")]
    pub npm_port: u16,

    #[arg(long)]
    pub npm_url: Option<String>,

    #[arg(long)]
    pub secure: bool,
}

impl NpmConnectionArgs {
    pub fn npm_host(&self) -> &str {
        self.npm_host.as_deref().unwrap_or(&self.host)
    }

    pub fn base_url(&self) -> String {
        self.npm_url
            .clone()
            .unwrap_or_else(|| format!("http://{}:{}", self.host, self.npm_port))
    }
}

#[derive(Args, Debug, Clone)]
pub struct StackDeployArgs {
    #[command(flatten)]
    pub portainer: PortainerConnectionArgs,

    #[arg(long, short = 'n', required = true)]
    pub name: String,

    #[arg(long, default_value = "local")]
    pub endpoint: String,
}

#[derive(Args, Debug, Clone)]
pub struct StackRemoveArgs {
    #[command(flatten)]
    pub portainer: PortainerConnectionArgs,

    #[arg(long, short = 'n', required = true)]
    pub name: String,

    #[arg(long, default_value = "local")]
    pub endpoint: String,
}

#[derive(Args, Debug, Clone)]
pub struct StackListArgs {
    #[command(flatten)]
    pub portainer: PortainerConnectionArgs,
}

#[derive(Subcommand, Debug)]
pub enum ProxyCommands {
    Enable(ProxyEnableArgs),

    List(ProxyListArgs),
}

#[derive(Args, Debug, Clone)]
pub struct ProxyEnableArgs {
    #[command(flatten)]
    pub npm: NpmConnectionArgs,

    #[arg(long, short = 's', value_delimiter = ',', required = true)]
    pub stack: Vec<String>,
}

#[derive(Args, Debug, Clone)]
pub struct ProxyListArgs {
    #[command(flatten)]
    pub npm: NpmConnectionArgs,
}

#[derive(Subcommand, Debug)]
pub enum CredsCommands {
    Set(CredsSetArgs),
    List,
    Remove(CredsRemoveArgs),
}

#[derive(Args, Debug)]
pub struct CredsSetArgs {
    #[arg(long, short = 'H')]
    pub host: String,

    #[arg(long, short = 's')]
    pub service: String,

    #[arg(long, short = 'u')]
    pub user: Option<String>,

    #[arg(long, short = 'p')]
    pub password: Option<String>,

    #[arg(long)]
    pub url: Option<String>,
}

#[derive(Args, Debug)]
pub struct CredsRemoveArgs {
    #[arg(long, short = 'H')]
    pub host: String,

    #[arg(long, short = 's')]
    pub service: String,
}

#[derive(Subcommand, Debug)]
pub enum TemplatesCommands {
    List(ListTemplatesArgs),
    Show(ShowTemplateArgs),
}

#[derive(Args, Debug)]
pub struct ListTemplatesArgs {
    #[command(flatten)]
    pub portainer: PortainerConnectionArgs,
}

#[derive(Args, Debug)]
pub struct ShowTemplateArgs {
    #[command(flatten)]
    pub portainer: PortainerConnectionArgs,

    #[arg(long, short = 'n', required = true)]
    pub name: String,
}
