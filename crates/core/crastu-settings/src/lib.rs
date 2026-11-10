//! One settings source; secrets behind a platform keychain trait.
//!
//! Settings are a flat map of dotted keys (`appearance.theme`, `voice.speed`) to JSON values,
//! saved atomically to one file. Typed access goes through serde. Secrets such as API keys never
//! touch the settings file: they live in a [`SecretStore`] (the macOS Keychain on desktop).

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("settings file is not valid JSON: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("setting {key:?} has the wrong type: {source}")]
    Type {
        key: String,
        source: serde_json::Error,
    },
}

pub type Result<T> = std::result::Result<T, SettingsError>;

#[derive(Debug)]
pub struct Settings {
    path: Option<PathBuf>,
    defaults: BTreeMap<String, Value>,
    values: BTreeMap<String, Value>,
}

impl Settings {
    /// In-memory settings (tests, previews).
    pub fn in_memory() -> Settings {
        Settings {
            path: None,
            defaults: BTreeMap::new(),
            values: BTreeMap::new(),
        }
    }

    /// Loads from `path`; a missing file means "all defaults".
    pub fn load(path: impl Into<PathBuf>) -> Result<Settings> {
        let path = path.into();
        let values = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(e) => return Err(e.into()),
        };
        Ok(Settings {
            path: Some(path),
            defaults: BTreeMap::new(),
            values,
        })
    }

    /// Registers a default, usually from the module registry.
    pub fn set_default(&mut self, key: &str, value: Value) {
        self.defaults.insert(key.to_string(), value);
    }

    /// The stored value, else the default, else `None`.
    pub fn get_value(&self, key: &str) -> Option<&Value> {
        self.values.get(key).or_else(|| self.defaults.get(key))
    }

    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        match self.get_value(key) {
            None => Ok(None),
            Some(v) => serde_json::from_value(v.clone())
                .map(Some)
                .map_err(|source| SettingsError::Type {
                    key: key.to_string(),
                    source,
                }),
        }
    }

    pub fn set<T: Serialize>(&mut self, key: &str, value: T) -> Result<()> {
        let v = serde_json::to_value(value)?;
        if self.defaults.get(key) == Some(&v) {
            self.values.remove(key);
        } else {
            self.values.insert(key.to_string(), v);
        }
        Ok(())
    }

    pub fn reset(&mut self, key: &str) {
        self.values.remove(key);
    }

    /// Keys the user changed from their defaults.
    pub fn changed_keys(&self) -> impl Iterator<Item = &str> {
        self.values.keys().map(String::as_str)
    }

    /// Writes the non-default values atomically (temp file, then rename).
    pub fn save(&self) -> Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        write_atomic(path, &serde_json::to_vec_pretty(&self.values)?)?;
        Ok(())
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(tmp, path)
}

/// Where API keys and tokens live. The desktop adapter uses the platform keychain.
pub trait SecretStore: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<String>>;
    fn set(&self, key: &str, secret: &str) -> Result<()>;
    fn delete(&self, key: &str) -> Result<()>;
}

/// A secret store that forgets everything on exit. For tests and previews only.
#[derive(Debug, Default)]
pub struct MemorySecretStore(Mutex<HashMap<String, String>>);

impl SecretStore for MemorySecretStore {
    fn get(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(key)
            .cloned())
    }
    fn set(&self, key: &str, secret: &str) -> Result<()> {
        self.0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(key.to_string(), secret.to_string());
        Ok(())
    }
    fn delete(&self, key: &str) -> Result<()> {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_values_and_reset() {
        let mut s = Settings::in_memory();
        s.set_default("appearance.theme", json!("graphite"));
        assert_eq!(
            s.get::<String>("appearance.theme").unwrap().as_deref(),
            Some("graphite")
        );
        s.set("appearance.theme", "paper").unwrap();
        assert_eq!(
            s.get::<String>("appearance.theme").unwrap().as_deref(),
            Some("paper")
        );
        assert_eq!(
            s.changed_keys().collect::<Vec<_>>(),
            vec!["appearance.theme"]
        );
        s.set("appearance.theme", "graphite").unwrap();
        assert_eq!(
            s.changed_keys().count(),
            0,
            "setting a default value stores nothing"
        );
        assert_eq!(s.get::<u32>("missing").unwrap(), None);
    }

    #[test]
    fn wrong_type_is_an_error() {
        let mut s = Settings::in_memory();
        s.set("voice.speed", "fast").unwrap();
        assert!(matches!(
            s.get::<f64>("voice.speed"),
            Err(SettingsError::Type { .. })
        ));
    }

    #[test]
    fn saves_and_loads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/settings.json");
        let mut s = Settings::load(&path).unwrap();
        s.set("appearance.density", "comfortable").unwrap();
        s.save().unwrap();
        let again = Settings::load(&path).unwrap();
        assert_eq!(
            again
                .get::<String>("appearance.density")
                .unwrap()
                .as_deref(),
            Some("comfortable")
        );
        assert!(!path.with_extension("tmp").exists());
    }

    #[test]
    fn memory_secret_store() {
        let store = MemorySecretStore::default();
        store.set("provider.anthropic", "sk-test").unwrap();
        assert_eq!(
            store.get("provider.anthropic").unwrap().as_deref(),
            Some("sk-test")
        );
        store.delete("provider.anthropic").unwrap();
        assert_eq!(store.get("provider.anthropic").unwrap(), None);
    }
}
