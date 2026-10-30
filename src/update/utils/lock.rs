use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// 锁文件中的包信息
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PackageLock {
    pub name: String,
    #[serde(default)]
    pub uuid: Option<String>,
    pub version: String,
    #[serde(rename = "type")]
    pub pkg_type: String,
    pub loader: Vec<Vec<String>>,
}

/// dytes.lock 文件结构
#[derive(Debug, Deserialize, Serialize, Default)]
pub struct DytesLock {
    pub package: Vec<PackageLock>,
}

/// 读取锁文件
pub fn read_lock() -> Result<DytesLock> {
    let lock_path = "dytes.lock";
    if !Path::new(lock_path).exists() {
        return Ok(DytesLock::default());
    }

    let content =
        fs::read_to_string(lock_path).with_context(|| format!("读取锁文件 {} 失败", lock_path))?;

    let lock: DytesLock =
        serde_json::from_str(&content).with_context(|| format!("解析锁文件 {} 失败", lock_path))?;

    Ok(lock)
}

/// 写入锁文件
pub fn write_lock(lock: &DytesLock) -> Result<()> {
    let lock_path = "dytes.lock";
    let content = serde_json::to_string_pretty(lock)?;
    fs::write(lock_path, content)?;
    println!("  锁文件已更新: {}", lock_path);
    Ok(())
}

/// 更新或添加包到锁文件
pub fn update_package_in_lock(
    uuid: Option<&str>,
    name: &str,
    version: &str,
    pkg_type: &str,
    loader: Vec<Vec<String>>,
) -> Result<()> {
    let mut lock = read_lock()?;

    let package_lock = PackageLock {
        name: name.to_string(),
        uuid: uuid.map(str::to_string),
        version: version.to_string(),
        pkg_type: pkg_type.to_string(),
        loader,
    };

    // 新记录按 uuid 去重；旧记录没有 uuid 时按 name 去重。
    lock.package.retain(|p| {
        if let Some(uuid) = uuid {
            p.uuid.as_deref() != Some(uuid) && p.name != name
        } else {
            p.name != name
        }
    });
    lock.package.push(package_lock);

    write_lock(&lock)
}

/// 按 name 或 uuid 查找锁文件记录（兼容旧锁文件）。
pub fn get_package_from_lock(key: &str) -> Result<Option<PackageLock>> {
    let lock = read_lock()?;
    Ok(lock
        .package
        .into_iter()
        .find(|p| p.name == key || p.uuid.as_deref() == Some(key)))
}

/// 从锁文件中获取包版本；key 可以是 name 或 uuid。
pub fn get_package_version_from_lock(key: &str) -> Result<Option<String>> {
    let lock = read_lock()?;
    Ok(lock
        .package
        .into_iter()
        .find(|p| p.name == key || p.uuid.as_deref() == Some(key))
        .map(|p| p.version))
}

/// 检查包是否在锁文件中；key 可以是 name 或 uuid。
pub fn is_package_in_lock(key: &str) -> Result<bool> {
    let lock = read_lock()?;
    Ok(lock
        .package
        .iter()
        .any(|p| p.name == key || p.uuid.as_deref() == Some(key)))
}

/// 从锁文件中移除包；key 可以是 name 或 uuid。
pub fn remove_package_from_lock(key: &str) -> Result<()> {
    let mut lock = read_lock()?;
    lock.package
        .retain(|p| p.name != key && p.uuid.as_deref() != Some(key));
    write_lock(&lock)
}

/// 获取锁文件中所有包的类型列表
pub fn get_packages_by_type(pkg_type: &str) -> Result<Vec<PackageLock>> {
    let lock = read_lock()?;
    Ok(lock
        .package
        .into_iter()
        .filter(|p| p.pkg_type == pkg_type)
        .collect())
}
