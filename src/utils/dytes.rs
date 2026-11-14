use crate::utils::checksum::FileChecksum;
use crate::utils::error::{DytesError, Result as SharedResult};
use anyhow::{Context, Result};
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

/// dytes.json 配置结构（标准格式）
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
    #[serde(default)]
    #[serde(rename = "type")]
    pub pkg_type: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    #[serde(rename = "dependencies-core")]
    pub dependencies_core: HashMap<String, String>,
    #[serde(default)]
    #[serde(rename = "dependencies-sys")]
    pub dependencies_sys: HashMap<String, String>,
    #[serde(default)]
    #[serde(rename = "dependencies-web")]
    pub dependencies_web: HashMap<String, String>,
    #[serde(default)]
    #[serde(rename = "dependencies-design-plugins")]
    pub dependencies_design_plugins: HashMap<String, String>,
    #[serde(default)]
    #[serde(rename = "dependencies-design-soft")]
    pub dependencies_design_soft: HashMap<String, String>,
    #[serde(default)]
    #[serde(rename = "dependencies-model")]
    pub dependencies_model: HashMap<String, String>,
    /// 测试环境依赖：按包类型分组，例如
    /// `{"sys": {"info_lib": "0.1.0"}, "web": {"setup": "0.2.0"}}`。
    /// `dytes test` 会把本地已安装的对应依赖复制到 `.dytes/env`。
    #[serde(default, rename = "test-dependency")]
    pub test_dependency: HashMap<String, HashMap<String, String>>,
    #[serde(default)]
    pub exports: HashMap<String, String>,
    #[serde(default, rename = "platform-exports")]
    pub platform_exports: HashMap<String, HashMap<String, String>>,
    #[serde(default)]
    pub init: Vec<String>,
    #[serde(default = "default_auto_update", rename = "auto_update")]
    pub auto_update: bool,
    #[serde(default)]
    pub checksums: HashMap<String, FileChecksum>,
}

fn default_auto_update() -> bool {
    true
}

/// 依赖类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DependencyType {
    Core, // 可执行文件
    Sys,  // 系统库
    Web,  // Web 资源
    DesignPlugins,
    DesignSoft,
    Model,
}

impl DependencyType {
    pub fn as_str(&self) -> &'static str {
        match self {
            DependencyType::Core => "core",
            DependencyType::Sys => "sys",
            DependencyType::Web => "web",
            DependencyType::DesignPlugins => "design_plugins",
            DependencyType::DesignSoft => "design_soft",
            DependencyType::Model => "model",
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "core" => Some(DependencyType::Core),
            "sys" | "system" => Some(DependencyType::Sys),
            "web" | "webs" => Some(DependencyType::Web),
            "design_plugins" | "design-plugins" | "plugin" | "plugins" => {
                Some(DependencyType::DesignPlugins)
            }
            "design_soft" | "design-soft" | "designsoft" => Some(DependencyType::DesignSoft),
            "model" | "models" => Some(DependencyType::Model),
            _ => None,
        }
    }

    /// `dytes.json` 中对应的依赖表键名。
    pub fn manifest_key(self) -> &'static str {
        match self {
            DependencyType::Core => "dependencies-core",
            DependencyType::Sys => "dependencies-sys",
            DependencyType::Web => "dependencies-web",
            DependencyType::DesignPlugins => "dependencies-design-plugins",
            DependencyType::DesignSoft => "dependencies-design-soft",
            DependencyType::Model => "dependencies-model",
        }
    }

    /// 远程清单目录。
    pub fn remote_dir(self) -> &'static str {
        match self {
            DependencyType::Core => "core",
            DependencyType::Sys => "system",
            DependencyType::Web => "webs",
            DependencyType::DesignPlugins => "design_plugins",
            DependencyType::DesignSoft => "design_soft",
            DependencyType::Model => "models",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            DependencyType::Core => "核心包",
            DependencyType::Sys => "系统库",
            DependencyType::Web => "Web 资源",
            DependencyType::DesignPlugins => "设计插件",
            DependencyType::DesignSoft => "设计软件",
            DependencyType::Model => "模型",
        }
    }
}

/// 依赖项信息。
#[derive(Debug, Clone)]
pub struct Dependency {
    pub name: String,
    /// 去掉 `?` / `optional:` 标记后的声明版本。
    pub version: String,
    pub dep_type: DependencyType,
    /// 可选依赖：缺失时只提示，不计入失败。
    pub optional: bool,
}

impl Dependency {
    pub fn new(name: impl Into<String>, raw_version: &str, dep_type: DependencyType) -> Self {
        let (version, optional) = parse_dependency_spec(raw_version);
        Self {
            name: name.into(),
            version,
            dep_type,
            optional,
        }
    }
}

/// 解析依赖声明：支持 `?1.0.0`、`1.0.0?`、`optional:1.0.0` 三种可选写法。
pub fn parse_dependency_spec(raw: &str) -> (String, bool) {
    let raw = raw.trim();
    if let Some(rest) = raw.strip_prefix("optional:") {
        return (rest.trim().to_string(), true);
    }
    if let Some(rest) = raw.strip_prefix('?') {
        return (rest.trim().to_string(), true);
    }
    if let Some(rest) = raw.strip_suffix('?') {
        return (rest.trim().to_string(), true);
    }
    (raw.to_string(), false)
}

/// 读取 dytes.json
pub fn read_dytes_config() -> Result<DytesConfig> {
    let config_path = "dytes.json";
    if !Path::new(config_path).exists() {
        anyhow::bail!("配置文件 dytes.json 不存在");
    }

    let content = fs::read_to_string(config_path)
        .with_context(|| format!("读取配置文件 {} 失败", config_path))?;

    let config: DytesConfig = serde_json::from_str(&content)
        .with_context(|| format!("解析配置文件 {} 失败", config_path))?;

    Ok(config)
}

/// 获取所有依赖（按类型、名称排序，保证解析顺序稳定）。
pub fn get_all_dependencies(config: &DytesConfig) -> Vec<Dependency> {
    let mut deps = Vec::new();

    let groups: [(DependencyType, &HashMap<String, String>); 6] = [
        (DependencyType::Core, &config.dependencies_core),
        (DependencyType::Sys, &config.dependencies_sys),
        (DependencyType::Web, &config.dependencies_web),
        (
            DependencyType::DesignPlugins,
            &config.dependencies_design_plugins,
        ),
        (DependencyType::DesignSoft, &config.dependencies_design_soft),
        (DependencyType::Model, &config.dependencies_model),
    ];

    for (dep_type, map) in groups {
        for (name, version) in map {
            deps.push(Dependency::new(name, version, dep_type));
        }
    }

    deps.sort_by(|a, b| {
        a.dep_type
            .cmp(&b.dep_type)
            .then_with(|| a.name.cmp(&b.name))
    });
    deps
}

/// 打印配置信息
pub fn print_config_info(config: &DytesConfig) {
    println!("应用信息:");
    println!("  名称: {}", config.name);
    println!("  版本: {}", config.version);
    if let Some(desc) = &config.description {
        println!("  描述: {}", desc);
    }
    if let Some(author) = &config.author {
        println!("  作者: {}", author);
    }
    if let Some(license) = &config.license {
        println!("  许可证: {}", license);
    }

    println!("\n依赖统计:");
    println!("  核心依赖 (应用): {} 个", config.dependencies_core.len());
    println!("  系统依赖 (库): {} 个", config.dependencies_sys.len());
    println!("  Web 依赖: {} 个", config.dependencies_web.len());
    println!(
        "  设计插件: {} 个",
        config.dependencies_design_plugins.len()
    );
    println!("  设计软件: {} 个", config.dependencies_design_soft.len());
    println!("  模型: {} 个", config.dependencies_model.len());
    if !config.test_dependency.is_empty() {
        let count: usize = config
            .test_dependency
            .values()
            .map(HashMap::len)
            .sum();
        println!("  测试依赖: {} 个", count);
    }

    if !config.exports.is_empty() {
        println!("\n导出文件:");
        for (source, dest) in &config.exports {
            println!("  {} -> {}", source, dest);
        }
    }

    if !config.init.is_empty() {
        println!("\n初始化脚本:");
        for script in &config.init {
            println!("  {}", script);
        }
    }
}

impl DytesConfig {
    pub fn read(path: &Path) -> SharedResult<Self> {
        Self::read_with(path, false)
    }

    /// 读取清单；`allow_no_exports` 为 true 时允许“仅清单”包。
    pub fn read_with(path: &Path, allow_no_exports: bool) -> SharedResult<Self> {
        let text = fs::read_to_string(path)
            .map_err(|e| DytesError::config(format!("读取 {} 失败: {e}", path.display())))?;
        let config: Self = serde_json::from_str(&text)
            .map_err(|e| DytesError::config(format!("解析 {} 失败: {e}", path.display())))?;
        config.validate_with_options(allow_no_exports)?;
        Ok(config)
    }

    pub fn validate(&self) -> SharedResult<()> {
        self.validate_with_options(false)
    }

    pub fn validate_with_options(&self, allow_no_exports: bool) -> SharedResult<()> {
        if self.name.trim().is_empty() {
            return Err(DytesError::config("dytes.json 缺少 name"));
        }
        if self.version.trim().is_empty() {
            return Err(DytesError::config("dytes.json 缺少 version"));
        }
        if self.pkg_type.as_deref().unwrap_or("").trim().is_empty() {
            return Err(DytesError::config("dytes.json 缺少 type"));
        }
        if !allow_no_exports && self.exports.is_empty() && self.platform_exports.is_empty() {
            return Err(DytesError::config(
                "dytes.json 缺少 exports / platform-exports",
            ));
        }
        for pkg_type in self.test_dependency.keys() {
            if DependencyType::from_name(pkg_type).is_none() {
                return Err(DytesError::config(format!(
                    "test-dependency 包含未知包类型: {pkg_type}"
                )));
            }
        }
        Ok(())
    }

    pub fn package_type(&self) -> SharedResult<PackageType> {
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

    #[test]
    fn parses_optional_dependency_markers() {
        assert_eq!(parse_dependency_spec("1.0.0"), ("1.0.0".into(), false));
        assert_eq!(parse_dependency_spec("?1.0.0"), ("1.0.0".into(), true));
        assert_eq!(parse_dependency_spec("1.0.0?"), ("1.0.0".into(), true));
        assert_eq!(
            parse_dependency_spec("optional:^1.0.0"),
            ("^1.0.0".into(), true)
        );
    }

    #[test]
    fn dependency_type_paths() {
        assert_eq!(DependencyType::Sys.remote_dir(), "system");
        assert_eq!(DependencyType::Web.remote_dir(), "webs");
        assert_eq!(
            DependencyType::from_name("models"),
            Some(DependencyType::Model)
        );
    }
}
