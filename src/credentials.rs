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
use tracing::{debug, info, warn};

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
            debug!(
                "Credentials file {} not found — starting empty",
                path.display()
            );
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
        info!("Credentials saved to {}", self.path.display());
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

impl Default for CredentialStore {
    fn default() -> Self {
        Self {
            path: default_credentials_path(),
            store: Store::default(),
        }
    }
}

pub fn default_credentials_path() -> PathBuf {
    dirs::state_dir()
        .map(|d| d.join("portctl").join("credentials.toml"))
        .unwrap_or_else(|| PathBuf::from("credentials.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn temp_store() -> (TempDir, PathBuf, CredentialStore) {
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("credentials.toml");
        let store = CredentialStore::load(&path).expect("failed to load store");
        (dir, path, store)
    }

    fn make_creds(user: &str, password: &str, url: Option<&str>) -> Credentials {
        Credentials {
            user: Some(user.to_string()),
            password: Some(password.to_string()),
            url: url.map(str::to_string),
        }
    }

    macro_rules! setup {
        ($dir:ident, $path:ident, $store:ident) => {
            let ($dir, $path, mut $store) = temp_store();
        };

        ($dir:ident, $path:ident, $store:ident, $creds:ident) => {
            setup!($dir, $path, $store);
            let $creds = make_creds("admin", "s3cr3t", Some("https://192.168.1.100:9443"));
        };
    }

    #[test]
    fn test_credentials_is_complete() {
        let c = make_creds("u", "p", None);
        assert!(c.is_complete());
    }

    #[test]
    fn test_credentials_is_incomplete() {
        let c = Credentials {
            user: Some("u".into()),
            password: None,
            url: None,
        };
        assert!(!c.is_complete());
    }

    #[test]
    fn test_credentials_is_all_none() {
        let c = Credentials::default();
        assert!(!c.is_complete());
    }

    #[test]
    fn test_store_get_unknown_host() {
        setup!(_dir, _path, store);
        assert!(store.get("10.0.0.1", "npm").is_none());
    }

    #[test]
    fn test_store_set_and_get() {
        setup!(_dir, _path, store, creds);
        store.set("10.0.0.1", "npm", creds.clone());
        let npm = store.get("10.0.0.1", "npm").expect("should exist");
        assert_eq!(npm.user, creds.user);
        assert_eq!(npm.password, creds.password);
        assert_eq!(npm.url, creds.url);
    }

    #[test]
    fn test_entries_empty() {
        setup!(_dir, _path, store);
        assert!(store.all_entries().is_empty());
    }

    #[test]
    fn test_entries_sorted() {
        setup!(_dir, _path, store);
        store.set("10.0.0.3", "npm", make_creds("u", "p", None));
        store.set("10.0.0.2", "npm", make_creds("u", "p", None));
        store.set("10.0.0.1", "npm", make_creds("u", "p", None));

        let entries = store.all_entries();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].0, "10.0.0.1");
        assert_eq!(entries[1].0, "10.0.0.2");
        assert_eq!(entries[2].0, "10.0.0.3");
    }

    #[test]
    fn test_reload_credentials() {
        setup!(_dir, path, store, creds);
        store.set("10.0.0.1", "npm", creds.clone());
        store.save().expect("save failed");

        let reloaded = CredentialStore::load(&path).expect("reload failed");
        let reload_creds = reloaded
            .get("10.0.0.1", "npm")
            .expect("missing after reload");
        assert_eq!(reload_creds.user, creds.user);
        assert_eq!(reload_creds.password, creds.password);
        assert_eq!(reload_creds.url, creds.url);
    }

    #[test]
    fn test_save_credentials() {
        let dir = TempDir::new().unwrap();
        let nested = dir.path().join("a").join("b").join("credentials.toml");
        let mut store = CredentialStore::load(&nested).expect("load failed");
        store.set("h", "s", make_creds("u", "p", None));
        store.save().expect("save into nested dirs failed");
        assert!(nested.exists());
    }

    #[test]
    fn test_load_nonexistent_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("noexist.toml");
        let store = CredentialStore::load(&path).expect("should succeed with empty store");
        assert!(store.all_entries().is_empty());
    }

    #[test]
    fn test_load_invalid_toml() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("bad.toml");
        std::fs::write(&path, b"[[not valid toml]]]]").unwrap();
        assert!(CredentialStore::load(&path).is_err());
    }

    #[test]
    fn test_path_given_at_load() {
        setup!(_dir, path, store, _creds);
        assert_eq!(store.path(), path.as_path());
    }
}
