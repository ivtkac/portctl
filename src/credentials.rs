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
//! url      = "https://192.168.1.100:9443"  # optional
//! ```

use crate::error::Error;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use tracing::{debug, info, warn};

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

    pub fn merge(&mut self, other: Credentials) {
        if let Some(u) = other.user {
            self.user = Some(u);
        }
        if let Some(p) = other.password {
            self.password = Some(p);
        }
        if let Some(u) = other.url {
            self.url = Some(u);
        }
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
                Error::Other(format!(
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
            .map_err(|e| Error::Other(format!("Failed to serialize credentials: {e}")))?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.path, &serialized)?;
        info!("Credentials saved to {}", self.path.display());
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn get(&self, host: &str, service: &str) -> Option<&Credentials> {
        self.store.hosts.get(host)?.get(service)
    }

    pub fn get_creds(&self, host: &str, service: &str) -> Option<&Credentials> {
        let creds = self.get(host, service)?;
        if !creds.is_complete() {
            warn!("Credentials for {service}@{host} are incomplete (missing user or password)");
        }
        Some(creds)
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

    pub fn set(&mut self, host: &str, service: &str, creds: Credentials) {
        self.slot_mut(host, service).clone_from(&creds);
    }

    pub fn patch(&mut self, host: &str, service: &str, creds: Credentials) {
        self.slot_mut(host, service).merge(creds);
    }

    pub fn remove(&mut self, host: &str, service: &str) -> bool {
        self.store
            .hosts
            .get_mut(host)
            .map(|services| services.remove(service).is_some())
            .unwrap_or(false)
    }

    fn slot_mut(&mut self, host: &str, service: &str) -> &mut Credentials {
        self.store
            .hosts
            .entry(host.to_string())
            .or_default()
            .entry(service.to_string())
            .or_default()
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

    #[test]
    fn test_credentials_is_complete() {
        assert!(make_creds("u", "p", None).is_complete());
    }

    #[test]
    fn test_credentials_is_incomplete() {
        assert!(
            !Credentials {
                user: Some("u".into()),
                ..Default::default()
            }
            .is_complete()
        );
    }

    #[test]
    fn test_credentials_is_all_none() {
        assert!(!Credentials::default().is_complete());
    }
    #[test]
    fn test_merge_overwrites_some_fields() {
        let mut base = make_creds("old_user", "old_pass", Some("https://old"));
        base.merge(Credentials {
            user: Some("new_user".into()),
            ..Default::default()
        });
        assert_eq!(base.user.as_deref(), Some("new_user"));
        assert_eq!(base.password.as_deref(), Some("old_pass")); // unchanged
        assert_eq!(base.url.as_deref(), Some("https://old")); // unchanged
    }

    #[test]
    fn test_store_get_unknown_host() {
        let (_, _, store) = temp_store();
        assert!(store.get("10.0.0.1", "npm").is_none());
    }

    #[test]
    fn test_store_set_and_get() {
        let (_, _, mut store) = temp_store();
        let creds = make_creds("user", "pass", Some("https://example.com"));
        store.set("10.0.0.1", "npm", creds.clone());
        let found = store.get("10.0.0.1", "npm").expect("should exist");
        assert_eq!(found.user, creds.user);
        assert_eq!(found.password, creds.password);
        assert_eq!(found.url, creds.url);
    }

    #[test]
    fn test_entries_empty() {
        let (_, _, store) = temp_store();
        assert!(store.all_entries().is_empty());
    }

    #[test]
    fn test_entries_sorted() {
        let (_, _, mut store) = temp_store();
        for host in ["10.0.0.3", "10.0.0.2", "10.0.0.1"] {
            store.set(host, "npm", make_creds("u", "p", None));
        }
        let entries = store.all_entries();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].0, "10.0.0.1");
        assert_eq!(entries[1].0, "10.0.0.2");
        assert_eq!(entries[2].0, "10.0.0.3");
    }

    #[test]
    fn test_reload_credentials() {
        let (_, path, mut store) = temp_store();
        let creds = make_creds("u", "p", None);
        store.set("10.0.0.1", "npm", creds.clone());
        store.save().expect("save failed");

        let reloaded = CredentialStore::load(&path).expect("reload failed");
        let found = reloaded
            .get("10.0.0.1", "npm")
            .expect("missing after reload");
        assert_eq!(found.user, creds.user);
        assert_eq!(found.password, creds.password);
        assert_eq!(found.url, creds.url);
    }

    #[test]
    fn test_save_creates_nested_dirs() {
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
        let store = CredentialStore::load(dir.path().join("noexist.toml"))
            .expect("should succeed with empty store");
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
        let (_, path, store) = temp_store();
        assert_eq!(store.path(), path.as_path());
    }
}
