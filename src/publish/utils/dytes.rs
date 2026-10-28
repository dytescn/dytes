use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// dytes.json 配置结构（标准格式）
#[derive(Debug, Deserialize, Serialize)]
pub struct DytesConfig {
    pub name: String,
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
    pub exports: HashMap<String, String>,
    #[serde(default, rename = "platform-exports")]
    pub platform_exports: HashMap<String, HashMap<String, String>>,
    #[serde(default)]
    pub init: Vec<String>,
}

/// 依赖类型
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DependencyType {
    Core, // 可执行文件
    Sys,  // 系统库
    Web,  // Web 资源
}

impl DependencyType {
    pub fn as_str(&self) -> &'static str {
        match self {
            DependencyType::Core => "core",
            DependencyType::Sys => "sys",
            DependencyType::Web => "web",
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "core" => Some(DependencyType::Core),
            "sys" => Some(DependencyType::Sys),
            "web" => Some(DependencyType::Web),
            _ => None,
        }
    }
}

/// 依赖项信息
#[derive(Debug, Clone)]
pub struct Dependency {
    pub name: String,
    pub version: String,
    pub dep_type: DependencyType,
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

/// 获取所有依赖
pub fn get_all_dependencies(config: &DytesConfig) -> Vec<Dependency> {
    let mut deps = Vec::new();

    for (name, version) in &config.dependencies_core {
        deps.push(Dependency {
            name: name.clone(),
            version: version.clone(),
            dep_type: DependencyType::Core,
        });
    }

    for (name, version) in &config.dependencies_sys {
        deps.push(Dependency {
            name: name.clone(),
            version: version.clone(),
            dep_type: DependencyType::Sys,
        });
    }

    for (name, version) in &config.dependencies_web {
        deps.push(Dependency {
            name: name.clone(),
            version: version.clone(),
            dep_type: DependencyType::Web,
        });
    }

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
    }
}

/// 检查依赖是否存在
pub fn has_dependency(config: &DytesConfig, name: &str) -> bool {
    config.dependencies_core.contains_key(name)
        || config.dependencies_sys.contains_key(name)
        || config.dependencies_web.contains_key(name)
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
