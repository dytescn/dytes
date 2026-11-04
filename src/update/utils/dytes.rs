use crate::shared::checksum::FileChecksum;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// dytes.json 配置结构（标准格式）
#[derive(Debug, Deserialize, Serialize)]
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
        match s {
            "core" => Some(DependencyType::Core),
            "sys" | "system" => Some(DependencyType::Sys),
            "web" | "webs" => Some(DependencyType::Web),
            "design_plugins" => Some(DependencyType::DesignPlugins),
            "design_soft" => Some(DependencyType::DesignSoft),
            "model" | "models" => Some(DependencyType::Model),
            _ => None,
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

/// 根据类型获取依赖
pub fn get_dependencies_by_type(
    config: &DytesConfig,
    dep_type: DependencyType,
) -> HashMap<String, String> {
    match dep_type {
        DependencyType::Core => config.dependencies_core.clone(),
        DependencyType::Sys => config.dependencies_sys.clone(),
        DependencyType::Web => config.dependencies_web.clone(),
        DependencyType::DesignPlugins => config.dependencies_design_plugins.clone(),
        DependencyType::DesignSoft => config.dependencies_design_soft.clone(),
        DependencyType::Model => config.dependencies_model.clone(),
    }
}

/// 检查依赖是否存在
pub fn has_dependency(config: &DytesConfig, name: &str) -> bool {
    config.dependencies_core.contains_key(name)
        || config.dependencies_sys.contains_key(name)
        || config.dependencies_web.contains_key(name)
        || config.dependencies_design_plugins.contains_key(name)
        || config.dependencies_design_soft.contains_key(name)
        || config.dependencies_model.contains_key(name)
}

/// 获取依赖版本
pub fn get_dependency_version(config: &DytesConfig, name: &str) -> Option<String> {
    if let Some(version) = config.dependencies_core.get(name) {
        return Some(version.clone());
    }
    if let Some(version) = config.dependencies_sys.get(name) {
        return Some(version.clone());
    }
    if let Some(version) = config.dependencies_web.get(name) {
        return Some(version.clone());
    }
    if let Some(version) = config.dependencies_design_plugins.get(name) {
        return Some(version.clone());
    }
    if let Some(version) = config.dependencies_design_soft.get(name) {
        return Some(version.clone());
    }
    if let Some(version) = config.dependencies_model.get(name) {
        return Some(version.clone());
    }
    None
}

/// 获取依赖类型
pub fn get_dependency_type(config: &DytesConfig, name: &str) -> Option<DependencyType> {
    if config.dependencies_core.contains_key(name) {
        return Some(DependencyType::Core);
    }
    if config.dependencies_sys.contains_key(name) {
        return Some(DependencyType::Sys);
    }
    if config.dependencies_web.contains_key(name) {
        return Some(DependencyType::Web);
    }
    if config.dependencies_design_plugins.contains_key(name) {
        return Some(DependencyType::DesignPlugins);
    }
    if config.dependencies_design_soft.contains_key(name) {
        return Some(DependencyType::DesignSoft);
    }
    if config.dependencies_model.contains_key(name) {
        return Some(DependencyType::Model);
    }
    None
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

/// 验证配置是否有效
pub fn validate_config(config: &DytesConfig) -> Result<()> {
    if config.name.is_empty() {
        anyhow::bail!("配置缺少 'name' 字段");
    }
    if config.version.is_empty() {
        anyhow::bail!("配置缺少 'version' 字段");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
