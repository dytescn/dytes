use crate::shared::error::{DytesError, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_ENDPOINT: &str = "https://s3.api.upyun.com";
pub const DEFAULT_BUCKET: &str = "packages";
pub const DEFAULT_REGION: &str = "us-east-1";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct S3Config {
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub bucket: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub access_key: Option<String>,
    #[serde(default)]
    pub secret_key: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolConfig {
    #[serde(default)]
    pub s3: S3Config,
    #[serde(default)]
    pub platform: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedS3 {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub s3: ResolvedS3,
    pub platform: String,
}

impl ToolConfig {
    pub fn path() -> PathBuf {
        if let Ok(path) = std::env::var("DYTES_CONFIG") {
            return PathBuf::from(path);
        }
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("dytes")
            .join("config.toml")
    }

    pub fn load() -> Result<Self> {
        Self::load_from(&Self::path())
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(path)
            .map_err(|e| DytesError::config(format!("读取 {} 失败: {e}", path.display())))?;
        let config: Self = toml::from_str(&text)?;
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&Self::path())
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self)?;
        fs::write(path, text)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    /// 合并环境变量与非敏感默认值。
    pub fn resolve(&self) -> ResolvedConfig {
        let s3 = &self.s3;
        let builtin = builtin_s3();
        ResolvedConfig {
            s3: ResolvedS3 {
                endpoint: env_or("DYTES_S3_ENDPOINT", &s3.endpoint).unwrap_or(builtin.endpoint),
                bucket: env_or("DYTES_S3_BUCKET", &s3.bucket).unwrap_or(builtin.bucket),
                region: env_or("DYTES_S3_REGION", &s3.region)
                    .unwrap_or_else(|| DEFAULT_REGION.to_string()),
                access_key: env_or("DYTES_S3_ACCESS_KEY", &s3.access_key).or(builtin.access_key),
                secret_key: env_or("DYTES_S3_SECRET_KEY", &s3.secret_key).or(builtin.secret_key),
            },
            platform: env_or("DYTES_PLATFORM", &self.platform)
                .unwrap_or_else(|| std::env::consts::OS.to_string()),
        }
    }

    pub fn get_key(&self, key: &str) -> Option<String> {
        match key {
            "s3.endpoint" => self.s3.endpoint.clone(),
            "s3.bucket" => self.s3.bucket.clone(),
            "s3.region" => self.s3.region.clone(),
            "s3.access_key" => self.s3.access_key.clone(),
            "s3.secret_key" => self.s3.secret_key.clone(),
            "platform" => self.platform.clone(),
            _ => None,
        }
    }

    pub fn set_key(&mut self, key: &str, value: &str) -> Result<()> {
        let value = Some(value.to_string());
        match key {
            "s3.endpoint" => self.s3.endpoint = value,
            "s3.bucket" => self.s3.bucket = value,
            "s3.region" => self.s3.region = value,
            "s3.access_key" => self.s3.access_key = value,
            "s3.secret_key" => self.s3.secret_key = value,
            "platform" => self.platform = value,
            _ => return Err(DytesError::args(format!("未知配置键: {key}"))),
        }
        Ok(())
    }

    pub fn unset_key(&mut self, key: &str) -> Result<()> {
        match key {
            "s3.endpoint" => self.s3.endpoint = None,
            "s3.bucket" => self.s3.bucket = None,
            "s3.region" => self.s3.region = None,
            "s3.access_key" => self.s3.access_key = None,
            "s3.secret_key" => self.s3.secret_key = None,
            "platform" => self.platform = None,
            _ => return Err(DytesError::args(format!("未知配置键: {key}"))),
        }
        Ok(())
    }
}

fn env_or(key: &str, configured: &Option<String>) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            configured
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
}

/// 构建时注入的默认 S3 配置（来自 UPYUN_* 环境变量）。
pub fn builtin_s3() -> ResolvedS3 {
    ResolvedS3 {
        endpoint: option_env!("DYTES_BUILTIN_UPYUN_ENDPOINT")
            .unwrap_or(DEFAULT_ENDPOINT)
            .to_string(),
        bucket: option_env!("DYTES_BUILTIN_UPYUN_BUCKET")
            .unwrap_or(DEFAULT_BUCKET)
            .to_string(),
        region: DEFAULT_REGION.to_string(),
        access_key: option_env!("DYTES_BUILTIN_UPYUN_API_KEY").map(str::to_string),
        secret_key: option_env!("DYTES_BUILTIN_UPYUN_API_SECRET").map(str::to_string),
    }
}

/// 运行时有效的 S3 配置：用户配置 / 环境变量优先，构建内置值兜底。
pub fn effective_s3() -> ResolvedS3 {
    ToolConfig::load()
        .map(|config| config.resolve().s3)
        .unwrap_or_else(|_| builtin_s3())
}

pub fn mask(value: Option<&str>) -> String {
    match value {
        None => "(未设置)".to_string(),
        Some("") => "(未设置)".to_string(),
        Some(_) => "****".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_unset_and_mask() {
        let mut config = ToolConfig::default();
        config
            .set_key("s3.endpoint", "https://example.com")
            .unwrap();
        config.set_key("s3.access_key", "AK").unwrap();
        assert_eq!(
            config.get_key("s3.endpoint").as_deref(),
            Some("https://example.com")
        );
        assert_eq!(mask(config.get_key("s3.access_key").as_deref()), "****");
        config.unset_key("s3.endpoint").unwrap();
        assert!(config.get_key("s3.endpoint").is_none());
        assert!(config.set_key("unknown", "x").is_err());
    }

    #[test]
    fn resolves_defaults() {
        let resolved = ToolConfig::default().resolve();
        let builtin = builtin_s3();
        assert_eq!(resolved.s3.endpoint, builtin.endpoint);
        assert_eq!(resolved.s3.bucket, builtin.bucket);
        assert_eq!(resolved.s3.region, DEFAULT_REGION);
    }

    #[test]
    fn saves_and_loads_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut config = ToolConfig::default();
        config.set_key("s3.bucket", "my-bucket").unwrap();
        config.save_to(&path).unwrap();
        let loaded = ToolConfig::load_from(&path).unwrap();
        assert_eq!(loaded.s3.bucket.as_deref(), Some("my-bucket"));
    }
}
