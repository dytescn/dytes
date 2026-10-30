use crate::shared::error::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PackageLock {
    pub name: String,
    #[serde(default)]
    pub uuid: Option<String>,
    pub version: String,
    #[serde(rename = "type")]
    pub pkg_type: String,
    #[serde(default)]
    pub loader: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct DytesLock {
    #[serde(default)]
    pub package: Vec<PackageLock>,
}

impl DytesLock {
    pub fn path(dir: &Path) -> PathBuf {
        dir.join("dytes.lock")
    }

    pub fn read(dir: &Path) -> Result<Self> {
        let path = Self::path(dir);
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(&path)?;
        let lock: Self = serde_json::from_str(&text)?;
        Ok(lock)
    }

    pub fn write(&self, dir: &Path) -> Result<()> {
        let path = Self::path(dir);
        fs::write(&path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn upsert(&mut self, package: PackageLock) {
        self.package.retain(|item| {
            if let Some(uuid) = package.uuid.as_deref() {
                item.uuid.as_deref() != Some(uuid) && item.name != package.name
            } else {
                item.name != package.name
            }
        });
        self.package.push(package);
    }

    pub fn find(&self, key: &str) -> Option<&PackageLock> {
        self.package
            .iter()
            .find(|item| item.name == key || item.uuid.as_deref() == Some(key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_and_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mut lock = DytesLock::read(dir.path()).unwrap();
        lock.upsert(PackageLock {
            name: "info_lib".into(),
            uuid: Some("11111111-1111-4111-8111-111111111111".into()),
            version: "0.1.0".into(),
            pkg_type: "sys".into(),
            loader: vec![],
        });
        lock.upsert(PackageLock {
            name: "info_lib".into(),
            uuid: Some("11111111-1111-4111-8111-111111111111".into()),
            version: "0.2.0".into(),
            pkg_type: "sys".into(),
            loader: vec![],
        });
        lock.write(dir.path()).unwrap();
        let loaded = DytesLock::read(dir.path()).unwrap();
        assert_eq!(loaded.package.len(), 1);
        assert_eq!(loaded.find("info_lib").unwrap().version, "0.2.0");
    }
}
