//! Persistent credential store backed by a TOML file.
//!
//! File format:
//! ```toml
//! [hosts.192.168.1.100.npm]
//! user     = "admin"
//! password = "password"
//! url      = "https://192.168.1.100:81"    # optional
//!
//! [hosts.192.168.1.100.portainer]
//! user     = "admin"
//! password = "password"
//! url      = "https://192.168.1.100:9443"    # optional
//!

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use tracing::warn;

use serde::{Deserialize, Serialize};

use crate::error::Error;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Credentials {
    pub user: Option<String>,
    pub password: Option<String>,
    pub url: Option<String>,
}

impl Credentials {
    pub fn is_complete(&self) -> bool {
        self.user.is_some() && self.password.is_some()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Store {
    #[serde(default)]
    hosts: HashMap<String, HashMap<String, Credentials>>,
}

pub struct CredentialStore {
    path: PathBuf,
    store: Store,
}

impl CredentialStore {
    pub fn load(path: impl Into<PathBuf>) -> Result<Self, Error> {
        let path = path.into();
        let store = if path.exists() {
            let raw = std::fs::read_to_string(&path)?;
            toml::from_str(&raw).map_err(|e| {
                Error::other(format!(
                    "Failed to parse credentials file {}: {e}",
                    path.display()
                ))
            })?
        } else {
            Store::default()
        };
        Ok(Self { path, store })
    }

    pub fn save(&self) -> Result<(), Error> {
        let serialized = toml::to_string_pretty(&self.store)
            .map_err(|e| Error::other(format!("Failed to serialize credentials: {e}")))?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.path, serialized)?;
        Ok(())
    }

    pub fn get(&self, host: &str, service: &str) -> Option<&Credentials> {
        self.store.hosts.get(host)?.get(service)
    }

    pub fn get_complete(&self, host: &str, service: &str) -> Option<&Credentials> {
        let creds = self.get(host, service)?;
        if !creds.is_complete() {
            warn!("Credentials for {service}@{host} are incomplete (missing user or password)");
        }
        Some(creds)
    }

    pub fn set(&mut self, host: &str, service: &str, creds: Credentials) {
        self.store
            .hosts
            .entry(host.to_string())
            .or_default()
            .insert(service.to_string(), creds);
    }

    pub fn patch(&mut self, host: &str, service: &str, creds: Credentials) {
        let entry = self
            .store
            .hosts
            .entry(host.to_string())
            .or_default()
            .entry(service.to_string())
            .or_default();

        if let Some(u) = creds.user {
            entry.user = Some(u);
        }

        if let Some(p) = creds.password {
            entry.password = Some(p);
        }

        if let Some(u) = creds.url {
            entry.url = Some(u);
        }
    }

    pub fn remove(&mut self, host: &str, service: &str) -> bool {
        self.store
            .hosts
            .get_mut(host)
            .map(|services| services.remove(service).is_some())
            .unwrap_or(false)
    }

    pub fn all_entries(&self) -> Vec<(&str, &str, &Credentials)> {
        let mut entries: Vec<_> = self
            .store
            .hosts
            .iter()
            .flat_map(|(host, services)| {
                services
                    .iter()
                    .map(move |(svc, creds)| (host.as_str(), svc.as_str(), creds))
            })
            .collect();

        entries.sort_by_key(|(h, s, _)| (*h, *s));
        entries
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub fn default_credentials_path() -> PathBuf {
    dirs::state_dir()
        .map(|d| d.join("portctl").join("credentials.toml"))
        .unwrap_or_else(|| PathBuf::from("credentials.toml"))
}
