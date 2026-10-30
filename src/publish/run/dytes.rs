use crate::publish::utils::dytes::DytesConfig;
use crate::publish::utils::loader::{download_from_s3, object_exists, upload_bytes, upload_file};
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::fs;
use std::path::Path;
use uuid::Uuid;

/// 根据 type 获取对应的远程路径前缀
fn get_remote_prefix(pkg_type: &str, name: &str) -> String {
    match pkg_type {
        "app" => format!("app/{}", name),
        "core" => format!("core/{}", name),
        "sys" => format!("system/{}", name),
        "web" => format!("webs/{}", name),
        "design_plugins" => format!("design_plugins/{}", name),
        "design_soft" => format!("design_soft/{}", name),
        "model" => format!("models/{}", name),
        _ => format!("core/{}", name),
    }
}

/// 根据 type 获取 packages.json 的远程路径
fn get_packages_remote_path(pkg_type: &str) -> String {
    match pkg_type {
        "app" => "app/packages.json".to_string(),
        "core" => "core/packages.json".to_string(),
        "sys" => "system/packages.json".to_string(),
        "web" => "webs/packages.json".to_string(),
        "design_plugins" => "design_plugins/packages.json".to_string(),
        "design_soft" => "design_soft/packages.json".to_string(),
        "model" => "models/packages.json".to_string(),
        _ => "core/packages.json".to_string(),
    }
}

/// 判断 S3 错误是否为对象不存在（首次发布时允许创建新的 packages.json）
fn is_not_found_error(error: &str) -> bool {
    let lower = error.to_lowercase();
    lower.contains("nosuchkey")
        || lower.contains("not found")
        || lower.contains("notfound")
        || lower.contains("404")
        || lower.contains("no such key")
}

/// 更新 packages.json
///
/// 读取失败时不再静默重建，避免一次网络故障把整个包索引覆盖成空表。
fn update_packages_json(
    uuid: &str,
    name: &str,
    version: &str,
    pkg_type: &str,
    bucket: &str,
    packages_remote_path: &str,
) -> Result<()> {
    use chrono::Utc;

    println!(
        "  从 S3 获取 packages.json: {}/{}",
        bucket, packages_remote_path
    );

    let mut packages_data: Value = match download_from_s3(bucket, packages_remote_path) {
        Ok(text) => serde_json::from_str(&text)
            .with_context(|| format!("解析远程 {} 失败，发布已中止", packages_remote_path))?,
        Err(e) if is_not_found_error(&e) => {
            println!("  远程 packages.json 不存在，创建新的");
            serde_json::json!({
                "packages": {},
                "updated_at": Utc::now().to_rfc3339()
            })
        }
        Err(e) => {
            match crate::publish::utils::loader::object_exists(bucket, packages_remote_path) {
                Ok(false) => {
                    println!("  远程 packages.json 不存在，创建新的");
                    serde_json::json!({
                        "packages": {},
                        "updated_at": Utc::now().to_rfc3339()
                    })
                }
                Ok(true) => {
                    bail!(
                        "读取远程 {} 失败（对象存在）: {}。为避免覆盖现有包索引，发布已中止。",
                        packages_remote_path,
                        e
                    );
                }
                Err(head_err) => {
                    bail!(
                        "检查远程 {} 失败: {}（原读取错误: {}）。发布已中止。",
                        packages_remote_path,
                        head_err,
                        e
                    );
                }
            }
        }
    };

    // 更新包版本：新索引以 uuid 为主键，同时兼容旧 name -> version 形式。
    if let Some(obj) = packages_data.as_object_mut() {
        let packages = obj.entry("packages").or_insert(serde_json::json!({}));

        let old_version = packages
            .get(uuid)
            .and_then(entry_version)
            .or_else(|| packages.get(name).and_then(entry_version))
            .or_else(|| {
                packages
                    .as_object()
                    .and_then(|map| map.values().find(|value| value_name(value) == Some(name)))
                    .and_then(entry_version)
            })
            .unwrap_or("未找到")
            .to_string();

        if let Some(pkg_obj) = packages.as_object_mut() {
            // 迁移：移除同名旧键（name -> version）或旧 uuid 条目，避免重复。
            pkg_obj
                .retain(|key, value| key != name && key != uuid && value_name(value) != Some(name));
            pkg_obj.insert(
                uuid.to_string(),
                serde_json::json!({
                    "name": name,
                    "version": version,
                    "type": pkg_type,
                    "updated_at": Utc::now().to_rfc3339(),
                }),
            );
            println!(
                "  包 {} ({}) 版本已更新: {} -> {}",
                name, uuid, old_version, version
            );
        }

        // 更新更新时间
        obj.insert(
            "updated_at".to_string(),
            serde_json::Value::String(Utc::now().to_rfc3339()),
        );
    }

    let content = serde_json::to_vec_pretty(&packages_data)?;

    if !upload_bytes(&content, bucket, packages_remote_path) {
        bail!("{} 上传失败", packages_remote_path);
    }

    println!("  packages.json 更新成功");
    Ok(())
}

/// packages.json 条目版本：兼容旧 `name -> "1.0.0"` 与新 `{version,...}`。
fn entry_version(value: &Value) -> Option<&str> {
    value
        .as_str()
        .or_else(|| value.get("version").and_then(Value::as_str))
}

fn value_name(value: &Value) -> Option<&str> {
    value.get("name").and_then(Value::as_str)
}

/// 确保 dytes.json 有 uuid：缺失时生成并写回源文件。
fn ensure_manifest_uuid(json_path: &Path, raw: &str) -> Result<String> {
    let mut value: Value = serde_json::from_str(raw).with_context(|| "解析 dytes.json 失败")?;
    if let Some(existing) = value.get("uuid").and_then(Value::as_str) {
        let trimmed = existing.trim();
        if !trimmed.is_empty() {
            let uuid = Uuid::parse_str(trimmed)
                .with_context(|| format!("dytes.json 的 uuid 非法: {trimmed}"))?;
            return Ok(uuid.to_string());
        }
    }

    let uuid = Uuid::new_v4().to_string();
    let object = value
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("dytes.json 不是 JSON 对象"))?;
    object.insert("uuid".to_string(), Value::String(uuid.clone()));
    fs::write(json_path, serde_json::to_string_pretty(&value)?)
        .with_context(|| format!("写回 uuid 到 {} 失败", json_path.display()))?;
    println!("  dytes.json 缺少 uuid，已生成并写回: {}", uuid);
    Ok(uuid)
}

/// 发布包
///
/// 发布顺序：导出文件 -> dytes.json -> packages.json。
/// 只有所有文件都上传成功后，才把新版本写进 packages.json，
/// 避免客户端在文件尚未上传完成时看到新版本。
pub fn publish(json_path: &Path) -> Result<()> {
    println!("读取配置文件...");

    // 1. 读取 dytes.json
    let content = fs::read_to_string(json_path)
        .with_context(|| format!("读取文件失败: {}", json_path.display()))?;

    let config: DytesConfig =
        serde_json::from_str(&content).with_context(|| "解析 dytes.json 失败")?;

    // 2. 校验必要字段
    let name = config.name.trim().to_string();
    if name.is_empty() {
        bail!("dytes.json 的 name 不能为空");
    }

    let version = config.version.trim().to_string();
    semver::Version::parse(&version)
        .with_context(|| format!("version 不是合法的 semver 版本号: {}", version))?;

    let pkg_type = config.pkg_type.as_deref().unwrap_or("");
    if !matches!(
        pkg_type,
        "app" | "core" | "sys" | "web" | "design_plugins" | "design_soft" | "model"
    ) {
        bail!(
            "不支持的包类型 '{}'，应为 app / core / sys / web / design_plugins / design_soft / model",
            pkg_type
        );
    }

    let uuid = ensure_manifest_uuid(json_path, &content)?;

    let platform = crate::publish::utils::platform::current_platform();
    let Some((exports, platform_tag)) =
        crate::publish::utils::platform::select_exports(&config, &platform)
    else {
        bail!("当前平台 '{}' 未在 platform-exports 中配置", platform);
    };

    println!("  名称: {}", name);
    println!("  UUID: {}", uuid);
    println!("  版本: {}", version);
    println!("  类型: {}", pkg_type);
    println!("  平台: {}", platform);
    println!("  导出文件数: {}", exports.len());
    println!();

    // 3. 确定远程路径
    let remote_prefix = get_remote_prefix(pkg_type, &name);
    let packages_remote_path = get_packages_remote_path(pkg_type);

    println!("远程路径前缀: {}", remote_prefix);
    println!("packages.json: {}", packages_remote_path);
    println!();

    let bucket = "packages";

    // 4. 先上传导出文件，全部成功后才发布版本
    let base_path =
        crate::publish::utils::platform::remote_base(&remote_prefix, &version, platform_tag);
    let json_dir = json_path.parent().unwrap_or(Path::new("."));

    let mut success_count = 0;
    let mut fail_count = 0;

    if exports.is_empty() {
        println!("没有导出文件，跳过文件上传");
    } else {
        println!("上传导出文件...");

        for (local_path, remote_name) in exports {
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

            let is_large_file = fs::metadata(&full_local_path)
                .map(|m| m.len() > 100 * 1024 * 1024)
                .unwrap_or(false);
            if is_large_file {
                match object_exists(bucket, &object_name) {
                    Ok(true) => {
                        println!("   大文件对象已存在，跳过重复上传");
                        success_count += 1;
                        continue;
                    }
                    Ok(false) => {}
                    Err(e) => println!("   检查对象是否存在失败，继续上传: {}", e),
                }
            }

            if upload_file(full_local_path.to_str().unwrap(), bucket, &object_name) {
                success_count += 1;
            } else {
                fail_count += 1;
            }
        }

        println!();
        println!(
            "上传完成: 成功 {} 个，失败 {} 个",
            success_count, fail_count
        );

        if fail_count > 0 {
            bail!("有 {} 个导出文件上传失败，已中止发布", fail_count);
        }
    }

    // 5. 上传 dytes.json
    println!("上传 dytes.json...");
    let dytes_remote_path = format!("{}/dytes.json", remote_prefix);
    if !upload_file(json_path.to_str().unwrap(), bucket, &dytes_remote_path) {
        bail!("dytes.json 上传失败，已中止发布");
    }
    println!("  dytes.json 上传成功");
    println!();

    // 6. 最后更新 packages.json，把版本指向已经上传完成的文件
    println!("更新 packages.json...");
    update_packages_json(
        &uuid,
        &name,
        &version,
        pkg_type,
        bucket,
        &packages_remote_path,
    )?;
    println!();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{get_packages_remote_path, get_remote_prefix};

    #[test]
    fn design_plugins_paths() {
        assert_eq!(
            get_remote_prefix("design_plugins", "cdrimex"),
            "design_plugins/cdrimex"
        );
        assert_eq!(
            get_packages_remote_path("design_plugins"),
            "design_plugins/packages.json"
        );
    }

    #[test]
    fn design_soft_paths() {
        assert_eq!(
            get_remote_prefix("design_soft", "vectorcraft"),
            "design_soft/vectorcraft"
        );
        assert_eq!(
            get_packages_remote_path("design_soft"),
            "design_soft/packages.json"
        );
    }
}
