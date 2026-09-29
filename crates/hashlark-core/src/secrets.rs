// SPDX-License-Identifier: GPL-3.0-or-later

//! Storage for credentials (provider passwords, API keys). Never SQLite.
//!
//! Desktop builds use the OS keychain. Headless servers and containers,
//! which usually have no keychain, use a file readable only by the owner.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::Mutex;

#[cfg(feature = "keychain")]
use crate::error::Error;
use crate::error::Result;

/// A key/value store for secrets. Calls may block (keychain access), so
/// async callers should use `spawn_blocking`.
pub trait SecretStore: Send + Sync + fmt::Debug {
    fn get(&self, key: &str) -> Result<Option<String>>;
    fn set(&self, key: &str, value: &str) -> Result<()>;
    fn delete(&self, key: &str) -> Result<()>;
}

/// Key under which a provider setting is stored.
pub fn provider_secret_key(provider_id: &str, field: &str) -> String {
    format!("provider/{provider_id}/{field}")
}

/// In-memory store, for tests and ephemeral use.
#[derive(Debug, Default)]
pub struct MemorySecretStore(Mutex<BTreeMap<String, String>>);

impl SecretStore for MemorySecretStore {
    fn get(&self, key: &str) -> Result<Option<String>> {
        Ok(self.0.lock().expect("lock").get(key).cloned())
    }

    fn set(&self, key: &str, value: &str) -> Result<()> {
        self.0
            .lock()
            .expect("lock")
            .insert(key.to_owned(), value.to_owned());
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<()> {
        self.0.lock().expect("lock").remove(key);
        Ok(())
    }
}

/// Secrets in a JSON file, with owner-only permissions on Unix. Used by the
/// headless server, where there is usually no OS keychain.
#[derive(Debug)]
pub struct FileSecretStore {
    path: PathBuf,
    lock: Mutex<()>,
}

impl FileSecretStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            lock: Mutex::new(()),
        }
    }

    fn read(&self) -> Result<BTreeMap<String, String>> {
        match std::fs::read(&self.path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(e) => Err(e.into()),
        }
    }

    fn write(&self, map: &BTreeMap<String, String>) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(map)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
        }
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

impl SecretStore for FileSecretStore {
    fn get(&self, key: &str) -> Result<Option<String>> {
        let _guard = self.lock.lock().expect("lock");
        Ok(self.read()?.get(key).cloned())
    }

    fn set(&self, key: &str, value: &str) -> Result<()> {
        let _guard = self.lock.lock().expect("lock");
        let mut map = self.read()?;
        map.insert(key.to_owned(), value.to_owned());
        self.write(&map)
    }

    fn delete(&self, key: &str) -> Result<()> {
        let _guard = self.lock.lock().expect("lock");
        let mut map = self.read()?;
        if map.remove(key).is_some() {
            self.write(&map)?;
        }
        Ok(())
    }
}

/// The OS keychain: Windows Credential Manager, macOS Keychain, or the
/// Secret Service on Linux.
#[cfg(feature = "keychain")]
#[derive(Debug)]
pub struct KeychainSecretStore {
    service: String,
}

#[cfg(feature = "keychain")]
impl KeychainSecretStore {
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    fn entry(&self, key: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(&self.service, key).map_err(|e| Error::Secrets(e.to_string()))
    }
}

#[cfg(feature = "keychain")]
impl SecretStore for KeychainSecretStore {
    fn get(&self, key: &str) -> Result<Option<String>> {
        match self.entry(key)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(Error::Secrets(e.to_string())),
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<()> {
        self.entry(key)?
            .set_password(value)
            .map_err(|e| Error::Secrets(e.to_string()))
    }

    fn delete(&self, key: &str) -> Result<()> {
        match self.entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(Error::Secrets(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exercise(store: &dyn SecretStore) {
        assert_eq!(store.get("a").unwrap(), None);
        store.set("a", "1").unwrap();
        store.set("b", "2").unwrap();
        store.set("a", "3").unwrap();
        assert_eq!(store.get("a").unwrap().as_deref(), Some("3"));
        store.delete("a").unwrap();
        store.delete("a").unwrap();
        assert_eq!(store.get("a").unwrap(), None);
        assert_eq!(store.get("b").unwrap().as_deref(), Some("2"));
    }

    #[test]
    fn memory_store() {
        exercise(&MemorySecretStore::default());
    }

    #[test]
    fn file_store_persists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.json");
        exercise(&FileSecretStore::new(&path));
        let reopened = FileSecretStore::new(&path);
        assert_eq!(reopened.get("b").unwrap().as_deref(), Some("2"));
    }

    #[test]
    fn provider_keys_are_namespaced() {
        assert_eq!(provider_secret_key("x", "password"), "provider/x/password");
    }
}
