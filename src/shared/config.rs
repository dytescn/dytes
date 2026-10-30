use crate::shared::error::{DytesError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// 包类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageType {
    App,
    Core,
    Sys,
    Web,
    DesignPlugins,
    DesignSoft,
    Model,
}

impl PackageType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::App => "app",
            Self::Core => "core",
            Self::Sys => "sys",
            Self::Web => "web",
            Self::DesignPlugins => "design_plugins",
            Self::DesignSoft => "design_soft",
            Self::Model => "model",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_lowercase().as_str() {
            "app" => Some(Self::App),
            "core" => Some(Self::Core),
            "sys" => Some(Self::Sys),
            "web" => Some(Self::Web),
            "design_plugins" | "design-plugins" | "plugin" | "plugins" => Some(Self::DesignPlugins),
            "design_soft" | "design-soft" => Some(Self::DesignSoft),
            "model" => Some(Self::Model),
            _ => None,
        }
    }
}

/// dytes.json 标准结构。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DytesConfig {
    pub name: String,
    #[serde(default)]
    pub uuid: Option<String>,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default, rename = "type")]
    pub pkg_type: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default, rename = "dependencies-core")]
    pub dependencies_core: HashMap<String, String>,
    #[serde(default, rename = "dependencies-sys")]
    pub dependencies_sys: HashMap<String, String>,
    #[serde(default, rename = "dependencies-web")]
    pub dependencies_web: HashMap<String, String>,
    #[serde(default, rename = "dependencies-design-plugins")]
    pub dependencies_design_plugins: HashMap<String, String>,
    #[serde(default, rename = "dependencies-design-soft")]
    pub dependencies_design_soft: HashMap<String, String>,
    #[serde(default, rename = "dependencies-model")]
    pub dependencies_model: HashMap<String, String>,
    #[serde(default)]
    pub exports: HashMap<String, String>,
    #[serde(default, rename = "platform-exports")]
    pub platform_exports: HashMap<String, HashMap<String, String>>,
    #[serde(default)]
    pub init: Vec<String>,
    #[serde(default = "default_auto_update", rename = "auto_update")]
    pub auto_update: bool,
}

fn default_auto_update() -> bool {
    true
}

impl DytesConfig {
    pub fn read(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .map_err(|e| DytesError::config(format!("读取 {} 失败: {e}", path.display())))?;
        let config: Self = serde_json::from_str(&text)
            .map_err(|e| DytesError::config(format!("解析 {} 失败: {e}", path.display())))?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            return Err(DytesError::config("dytes.json 缺少 name"));
        }
        if self.version.trim().is_empty() {
            return Err(DytesError::config("dytes.json 缺少 version"));
        }
        if self.pkg_type.as_deref().unwrap_or("").trim().is_empty() {
            return Err(DytesError::config("dytes.json 缺少 type"));
        }
        if self.exports.is_empty() && self.platform_exports.is_empty() {
            return Err(DytesError::config(
                "dytes.json 缺少 exports / platform-exports",
            ));
        }
        Ok(())
    }

    pub fn package_type(&self) -> Result<PackageType> {
        self.pkg_type
            .as_deref()
            .and_then(PackageType::parse)
            .ok_or_else(|| {
                DytesError::config(format!(
                    "未知包类型: {}",
                    self.pkg_type.as_deref().unwrap_or("")
                ))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> &'static str {
        r#"{
            "name": "info_lib",
            "version": "0.1.0",
            "type": "sys",
            "platform-exports": {"windows": {"./target/release/info_lib.dll": "info_lib.dll"}}
        }"#
    }

    #[test]
    fn parses_manifest() {
        let config: DytesConfig = serde_json::from_str(manifest()).unwrap();
        config.validate().unwrap();
        assert_eq!(config.package_type().unwrap(), PackageType::Sys);
    }

    #[test]
    fn rejects_missing_fields() {
        let config: DytesConfig =
            serde_json::from_str(r#"{"name":"x","version":"1.0.0","type":"sys"}"#).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn accepts_type_aliases() {
        assert_eq!(
            PackageType::parse("plugin"),
            Some(PackageType::DesignPlugins)
        );
        assert_eq!(
            PackageType::parse("design-plugins"),
            Some(PackageType::DesignPlugins)
        );
        assert_eq!(
            PackageType::parse("design-soft"),
            Some(PackageType::DesignSoft)
        );
        assert_eq!(PackageType::parse("unknown"), None);
    }
}
