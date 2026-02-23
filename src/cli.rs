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
pub struct StackRemoveArgs {
    #[arg(long, short = 'H', env = "PORTAINER_HOST")]
    pub host: String,

    #[arg(long, short = 'u', env = "PORTAINER_USER")]
    pub user: Option<String>,

    #[arg(long, short = 'p', env = "PORTAINER_PASSWORD")]
    pub password: Option<String>,

    #[arg(long, default_value = "9443")]
    pub portainer_port: u16,

    #[arg(long, short = 'n', required = true)]
    pub name: String,

    #[arg(long, default_value = "local")]
    pub endpoint: String,

    #[arg(long)]
    pub portainer_url: Option<String>,

    #[arg(long)]
    pub secure: bool,
}

impl StackRemoveArgs {
    pub fn portainer_base_url(&self) -> String {
        match &self.portainer_url {
            Some(url) => url.clone(),
            None => format!("https://{}:{}", self.host, self.portainer_port),
        }
    }
}

#[derive(Args, Debug, Clone)]
pub struct StackDeployArgs {
    #[arg(long, short = 'H', env = "PORTAINER_HOST")]
    pub host: String,

    #[arg(long, short = 'u', env = "PORTAINER_USER")]
    pub user: Option<String>,

    #[arg(long, short = 'p', env = "PORTAINER_PASSWORD")]
    pub password: Option<String>,

    #[arg(long, default_value = "9443")]
    pub portainer_port: u16,

    #[arg(long, short = 'n', required = true)]
    pub name: String,

    #[arg(long, default_value = "local")]
    pub endpoint: String,

    #[arg(long)]
    pub portainer_url: Option<String>,

    #[arg(long)]
    pub secure: bool,
}

impl StackDeployArgs {
    pub fn portainer_base_url(&self) -> String {
        match &self.portainer_url {
            Some(url) => url.clone(),
            None => format!("https://{}:{}", self.host, self.portainer_port),
        }
    }
}

#[derive(Args, Debug, Clone)]
pub struct StackListArgs {
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

impl StackListArgs {
    pub fn portainer_base_url(&self) -> String {
        match &self.portainer_url {
            Some(url) => url.clone(),
            None => format!("https://{}:{}", self.host, self.portainer_port),
        }
    }
}

#[derive(Subcommand, Debug)]
pub enum ProxyCommands {
    Enable(ProxyEnableArgs),

    List(ProxyListArgs),
}

#[derive(Args, Debug, Clone)]
pub struct ProxyEnableArgs {
    #[arg(long, short = 'H', env = "PORTAINER_HOST")]
    pub host: String,

    #[arg(long, short = 's', value_delimiter = ',', required = true)]
    pub stack: Vec<String>,

    #[arg(long)]
    pub npm_host: Option<String>,

    #[arg(long, default_value = "81")]
    pub npm_port: u16,

    #[arg(long)]
    pub npm_url: Option<String>,

    #[arg(long)]
    pub secure: bool,
}

impl ProxyEnableArgs {
    pub fn npm_host(&self) -> &str {
        self.npm_host.as_deref().unwrap_or(&self.host)
    }

    pub fn npm_base_url(&self) -> String {
        match &self.npm_url {
            Some(url) => url.clone(),
            None => format!("http://{}:{}", self.host, self.npm_port),
        }
    }
}

#[derive(Args, Debug, Clone)]
pub struct ProxyListArgs {
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

impl ProxyListArgs {
    pub fn npm_host(&self) -> &str {
        self.npm_host.as_deref().unwrap_or(&self.host)
    }

    pub fn npm_base_url(&self) -> String {
        match &self.npm_url {
            Some(url) => url.clone(),
            None => format!("http://{}:{}", self.host, self.npm_port),
        }
    }
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

#[derive(Args, Debug)]
pub struct ShowTemplateArgs {
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

    #[arg(long, short = 'n', required = true)]
    pub name: String,

    #[arg(long)]
    pub secure: bool,
}

impl ListTemplatesArgs {
    pub fn portainer_base_url(&self) -> String {
        match &self.portainer_url {
            Some(url) => url.clone(),
            None => format!("https://{}:{}", self.host, self.portainer_port),
        }
    }
}

impl ShowTemplateArgs {
    pub fn portainer_base_url(&self) -> String {
        match &self.portainer_url {
            Some(url) => url.clone(),
            None => format!("https://{}:{}", self.host, self.portainer_port),
        }
    }
}
