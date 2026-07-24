use anyhow::{Result, Context};
use std::fs;
use std::path::Path;
use crate::utils::loader::upload_file;
use crate::utils::dytes::DytesConfig;

/// 根据 type 获取对应的远程路径前缀
fn get_remote_prefix(pkg_type: &str, name: &str) -> String {
    match pkg_type {
        "app" => format!("app/{}", name),
        "core" => format!("core/{}", name),
        "sys" => format!("system/{}", name),
        "web" => format!("webs/{}", name),
        _ => {
            println!("  未知类型 '{}'，默认使用 core", pkg_type);
            format!("core/{}", name)
        }
    }
}

/// 根据 type 获取 packages.json 的远程路径
fn get_packages_remote_path(pkg_type: &str) -> String {
    match pkg_type {
        "app" => "app/packages.json".to_string(),
        "core" => "core/packages.json".to_string(),
        "sys" => "system/packages.json".to_string(),
        "web" => "webs/packages.json".to_string(),
        _ => "core/packages.json".to_string(),
    }
}

/// 根据 type 获取 packages.json 的 URL
fn get_packages_url(pkg_type: &str) -> String {
    let base_url = "https://packages.funxdata.com";
    match pkg_type {
        "app" => format!("{}/app/packages.json", base_url),
        "core" => format!("{}/core/packages.json", base_url),
        "sys" => format!("{}/system/packages.json", base_url),
        "web" => format!("{}/webs/packages.json", base_url),
        _ => format!("{}/core/packages.json", base_url),
    }
}

/// 更新 packages.json
fn update_packages_json(
    name: &str,
    version: &str,
    pkg_type: &str,
    packages_url: &str,
    bucket: &str,
    packages_remote_path: &str,
) -> Result<()> {
    use chrono::Utc;
    use serde_json::Value;
    use reqwest::blocking::Client;
    use std::fs;
    
    // 同步下载远程 packages.json
    let mut packages_data: Value = match Client::new().get(packages_url).send() {
        Ok(response) => {
            if response.status().is_success() {
                match response.text() {
                    Ok(text) => {
                        serde_json::from_str(&text).unwrap_or_else(|_| {
                            serde_json::json!({
                                "packages": {},
                                "updated_at": Utc::now().to_rfc3339()
                            })
                        })
                    }
                    Err(_) => {
                        println!("  读取响应内容失败，创建新的");
                        serde_json::json!({
                            "packages": {},
                            "updated_at": Utc::now().to_rfc3339()
                        })
                    }
                }
            } else {
                println!("  远程 packages.json 不存在 (HTTP {}), 创建新的", response.status());
                serde_json::json!({
                    "packages": {},
                    "updated_at": Utc::now().to_rfc3339()
                })
            }
        }
        Err(e) => {
            println!("  远程 packages.json 不存在: {}, 创建新的", e);
            serde_json::json!({
                "packages": {},
                "updated_at": Utc::now().to_rfc3339()
            })
        }
    };
    
    // 更新包版本
    if let Some(obj) = packages_data.as_object_mut() {
        // 获取或创建 packages 对象
        let packages = obj.entry("packages").or_insert(serde_json::json!({}));
        
        // 先获取旧版本（不可变借用）
        let old_version = packages
            .get(name)
            .and_then(|v| v.as_str())
            .unwrap_or("未找到")
            .to_string();
        
        // 然后进行可变借用插入新版本
        if let Some(pkg_obj) = packages.as_object_mut() {
            pkg_obj.insert(name.to_string(), serde_json::Value::String(version.to_string()));
            println!("  包 {} 版本已更新: {} -> {}", name, old_version, version);
        }
        
        // 更新更新时间
        obj.insert("updated_at".to_string(), serde_json::Value::String(Utc::now().to_rfc3339()));
    }
    
    // 保存到临时文件
    let temp_path = "packages_updated.json";
    let content = serde_json::to_string_pretty(&packages_data)?;
    fs::write(temp_path, content)?;
    
    // 上传
    let result = upload_file(temp_path, bucket, packages_remote_path);
    
    // 清理临时文件
    let _ = fs::remove_file(temp_path);
    
    if result {
        Ok(())
    } else {
        anyhow::bail!("packages.json 上传失败")
    }
}


/// 发布包
pub fn publish(json_path: &Path) -> Result<()> {
    println!("读取配置文件...");
    
    // 1. 读取 dytes.json
    let content = fs::read_to_string(json_path)
        .with_context(|| format!("读取文件失败: {}", json_path.display()))?;
    
    let config: DytesConfig = serde_json::from_str(&content)
        .with_context(|| "解析 dytes.json 失败")?;
    
    // 获取必要字段
    let name = config.name;
    let version = config.version;
    let pkg_type = config.pkg_type.as_deref().unwrap_or("core");
    let exports = config.exports;
    
    println!("  名称: {}", name);
    println!("  版本: {}", version);
    println!("  类型: {}", pkg_type);
    println!("  导出文件数: {}", exports.len());
    println!();
    
    // 2. 确定远程路径
    let remote_prefix = get_remote_prefix(pkg_type, &name);
    let packages_remote_path = get_packages_remote_path(pkg_type);
    let packages_url = get_packages_url(pkg_type);
    
    println!("远程路径前缀: {}", remote_prefix);
    println!("packages.json: {}", packages_remote_path);
    println!();
    
    let bucket = "packages";
    
    // 3. 上传 dytes.json
    println!("上传 dytes.json...");
    let dytes_remote_path = format!("{}/dytes.json", remote_prefix);
    if !upload_file(json_path.to_str().unwrap(), bucket, &dytes_remote_path) {
        eprintln!("  dytes.json 上传失败");
    } else {
        println!("  dytes.json 上传成功");
    }
    println!();
    
    // 4. 更新 packages.json
    println!("更新 packages.json...");
    if let Err(e) = update_packages_json(&name, &version, pkg_type, &packages_url, bucket, &packages_remote_path) {
        eprintln!("  packages.json 更新失败: {}", e);
    } else {
        println!("  packages.json 更新成功");
    }
    println!();
    
    // 5. 上传导出文件
    if exports.is_empty() {
        println!("没有导出文件，跳过");
        return Ok(());
    }
    
    println!("上传导出文件...");
    let base_path = format!("{}/{}/", remote_prefix, version);
    let json_dir = json_path.parent().unwrap_or(Path::new("."));
    
    let mut success_count = 0;
    let mut fail_count = 0;
    
    for (local_path, remote_name) in &exports {
        // 处理本地路径
        let local_path_clean = local_path.trim_start_matches("./");
        let full_local_path = json_dir.join(local_path_clean);
        
        // 检查本地文件是否存在
        if !full_local_path.exists() {
            eprintln!("  本地文件不存在: {}", full_local_path.display());
            fail_count += 1;
            continue;
        }
        
        // 处理远程文件名
        let remote_name_clean = remote_name.trim_start_matches("./");
        let object_name = format!("{}{}", base_path, remote_name_clean);
        
        println!("  上传: {} -> {}", full_local_path.display(), object_name);
        
        if upload_file(full_local_path.to_str().unwrap(), bucket, &object_name) {
            success_count += 1;
        } else {
            fail_count += 1;
        }
    }
    
    println!();
    println!("上传完成: 成功 {} 个，失败 {} 个", success_count, fail_count);
    
    if fail_count > 0 {
        anyhow::bail!("部分文件上传失败");
    }
    
    Ok(())
}