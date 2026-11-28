use crate::utils::checksum::verify_download;
use crate::utils::dytes::DytesConfig;
use crate::update::env::get_app_dir;
use crate::update::executable;
use crate::update::loader::download_file;
use crate::update::lock::{get_package_version_from_lock, update_package_in_lock};
use crate::utils::version::{check_version, clean_declared as parse_version};
use anyhow::{Context, Result};
use std::path::Path;


/// 判断是否为 HTML 文件
fn is_html_file(filename: &str) -> bool {
    let ext = Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());
    matches!(ext.as_deref(), Some("html") | Some("htm"))
}

/// 获取文件名（不含路径）
fn get_filename_from_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}

/// 读取远程 app/<name>/dytes.json（项目只有 dytes.json，没有 packages.json）
pub async fn fetch_remote_app_info(name: &str) -> Result<(DytesConfig, String)> {
    let path = format!("app/{}/dytes.json", name);
    let text = crate::update::loader::fetch_text(&path)
        .await
        .with_context(|| format!("下载 {} 失败", path))?;
    let config: DytesConfig =
        serde_json::from_str(&text).with_context(|| format!("解析 {} 失败", path))?;
    Ok((config, text))
}

/// 更新应用本身（基于锁文件版本与远程版本比较）
/// 合并远程配置与本地配置：远程配置优先，本地额外的依赖项保留
fn merge_local_dependencies(local_path: &Path, remote_text: &str) -> Result<String> {
    let mut remote: serde_json::Value = serde_json::from_str(remote_text)?;
    if let Ok(local_text) = std::fs::read_to_string(local_path) {
        if let Ok(local) = serde_json::from_str::<serde_json::Value>(&local_text) {
            for key in ["auto_update", "update_check_after"] {
                if let Some(field) = local.get(key) {
                    if remote.get(key).is_none() {
                        if let Some(obj) = remote.as_object_mut() {
                            obj.insert(key.to_string(), field.clone());
                        }
                    }
                }
            }

            for key in [
                "dependencies-core",
                "dependencies-sys",
                "dependencies-web",
                "dependencies-model",
            ] {
                if let Some(local_map) = local.get(key).and_then(|v| v.as_object()) {
                    let remote_obj = remote
                        .as_object_mut()
                        .ok_or_else(|| anyhow::anyhow!("远程 dytes.json 不是对象"))?;
                    let entry = remote_obj
                        .entry(key.to_string())
                        .or_insert_with(|| serde_json::json!({}));
                    if let Some(remote_map) = entry.as_object_mut() {
                        for (k, v) in local_map {
                            remote_map.entry(k.clone()).or_insert_with(|| v.clone());
                        }
                    }
                }
            }
        }
    }
    Ok(serde_json::to_string_pretty(&remote)?)
}

pub async fn update_app_info(
    name: String,
    config_version: String,
    sync_online: bool,
) -> Result<()> {
    println!("开始检查应用更新: {}", name);

    if !sync_online {
        if let Ok(Some(lock_version)) = get_package_version_from_lock(&name) {
            if parse_version(&lock_version) == parse_version(&config_version) {
                println!("  离线模式：应用已是最新 (v{})，跳过线上检查", lock_version);
                return Ok(());
            }
        }
    }

    // 1. 获取锁文件中记录的已安装版本
    let lock_version = get_package_version_from_lock(&name)
        .ok()
        .flatten()
        .and_then(|v| parse_version(&v));

    println!("  锁文件中的版本: {:?}", lock_version);

    // 2. 获取远程 app/<name>/dytes.json（项目没有 packages.json）
    let (remote_app_info, remote_app_info_text) = fetch_remote_app_info(&name).await?;
    let remote_version_parsed =
        parse_version(&remote_app_info.version).unwrap_or_else(|| remote_app_info.version.clone());
    println!("  远程应用版本: {}", remote_version_parsed);

    // 3. 判断是否需要更新
    // 如果锁文件版本不存在，或锁文件版本低于远程版本，则更新
    let need_update = match lock_version {
        None => {
            println!("  锁文件中无该包记录，视为首次安装，需要更新");
            true
        }
        Some(local) => {
            if local == remote_version_parsed {
                println!(
                    "  已是最新版本 (本地 {} == 远程 {})",
                    local, remote_version_parsed
                );
                false
            } else if check_version(&local, &remote_version_parsed) {
                println!(
                    "  需要更新: 本地 {} -> 远程 {}",
                    local, remote_version_parsed
                );
                true
            } else {
                println!(
                    "  本地版本 {} 高于远程 {}，无需更新",
                    local, remote_version_parsed
                );
                false
            }
        }
    };

    if !need_update {
        return Ok(());
    }

    // 3.1 仅清单应用：没有产物，只写回清单并登记版本。
    if remote_app_info.exports.is_empty() && remote_app_info.platform_exports.is_empty() {
        let app_dir = get_app_dir();
        std::fs::create_dir_all(&app_dir).context("创建应用目录失败")?;
        let local_config = Path::new(&app_dir).join("dytes.json");
        let merged_text = merge_local_dependencies(&local_config, &remote_app_info_text)
            .unwrap_or_else(|_| remote_app_info_text.clone());
        std::fs::write(&local_config, &merged_text)
            .with_context(|| format!("更新本地 dytes.json 失败: {}", local_config.display()))?;
        update_package_in_lock(
            remote_app_info.uuid.as_deref(),
            &name,
            &remote_app_info.version,
            Some(&config_version),
            "app",
            Vec::new(),
        )?;
        println!("  仅清单应用已安装: {} v{}", name, remote_app_info.version);
        return Ok(());
    }

    // 4. 远程清单已获取
    let platform = crate::utils::platform::current_platform();
    let Some((exports, platform_tag)) =
        crate::utils::platform::select_exports(&remote_app_info, &platform)
    else {
        anyhow::bail!("包 {} 未发布平台 {}", name, platform);
    };
    let base_path = crate::utils::platform::remote_base(
        &format!("app/{}", name),
        &remote_app_info.version,
        platform_tag,
    );

    // 平台专用清单必须带有当前平台的可执行后缀，避免拼出 `linux/dytes`
    // 这类不存在的下载路径；旧清单（仅 exports）只提示，保持兼容。
    match crate::utils::platform::validate_platform_exports(exports, &platform) {
        Ok(()) => {}
        Err(message) if platform_tag.is_some() => {
            anyhow::bail!("应用 {} 的导出文件与当前平台不匹配: {message}", name)
        }
        Err(message) => println!("  警告: {message}"),
    }

    println!("  远程版本: {}", remote_app_info.version);
    println!("  平台: {}", platform);
    println!("  导出文件: {:?}", exports);

    // 5. 获取应用目录
    let app_dir = get_app_dir();
    std::fs::create_dir_all(&app_dir).context("创建应用目录失败")?;
    println!("  应用目录: {}", app_dir);

    // 6. 下载导出的文件（失败即中止）
    for dest_filename in exports.values() {
        let is_html = is_html_file(dest_filename);
        let target_dir = if is_html {
            let web_dir = format!("{}/web", app_dir);
            std::fs::create_dir_all(&web_dir).context("创建 web 目录失败")?;
            web_dir
        } else {
            app_dir.clone()
        };

        let local_filename = get_filename_from_path(dest_filename);
        let remote_filename = local_filename.clone();

        let s3_path = format!("{}{}", base_path, remote_filename);
        let dest_path = format!("{}/{}", target_dir, local_filename);

        println!("  下载: {} -> {}", s3_path, dest_path);

        if !download_file(&s3_path, &target_dir).await {
            anyhow::bail!("下载 {} 失败", remote_filename);
        }

        let downloaded_path = format!("{}/{}", target_dir, remote_filename);
        if downloaded_path != dest_path && Path::new(&downloaded_path).exists() {
            if Path::new(&dest_path).exists() {
                std::fs::remove_file(&dest_path).context("删除旧目标文件失败")?;
            }
            std::fs::rename(&downloaded_path, &dest_path).context("重命名下载文件失败")?;
            println!("  重命名: {} -> {}", remote_filename, local_filename);
        }

        verify_download(
            Path::new(&dest_path),
            dest_filename,
            &remote_app_info.checksums,
        )?;
        if !is_html {
            executable::install_command_alias(Path::new(&target_dir), &local_filename)?;
        }
        println!("  下载完成: {}", local_filename);
    }

    // 7. 写回远程配置，保留本地额外依赖（core / sys / web / model）。
    // design_plugins / design_soft 不写入应用 dytes.json，只记录在 dytes.lock。
    let local_config = Path::new(&app_dir).join("dytes.json");
    let merged_text = merge_local_dependencies(&local_config, &remote_app_info_text)
        .unwrap_or_else(|_| remote_app_info_text.clone());
    std::fs::write(&local_config, &merged_text)
        .with_context(|| format!("更新本地 dytes.json 失败: {}", local_config.display()))?;
    println!("  已更新本地配置: {}", local_config.display());

    // 8. 构建 loader 数据
    let mut loader = Vec::new();
    for (source, dest) in exports {
        loader.push(vec![source.clone(), dest.clone()]);
    }

    // 9. 写入锁文件
    update_package_in_lock(
        remote_app_info.uuid.as_deref(),
        &name,
        &remote_app_info.version,
        Some(&config_version),
        "app",
        loader,
    )
    .context("更新锁文件失败")?;

    println!("  应用更新完成: {} v{}", name, remote_app_info.version);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::merge_local_dependencies;

    #[test]
    fn merge_keeps_manifest_dependencies_but_drops_design_packages() {
        let dir = tempfile::tempdir().unwrap();
        let local_path = dir.path().join("dytes.json");
        let local = serde_json::json!({
            "auto_update": false,
            "dependencies-sys": { "chatllm": "1.0.11" },
            "dependencies-design-plugins": { "cdr_check_txt": "0.2.0" },
            "dependencies-design-soft": { "vectorcraft": "0.17.0" }
        });
        std::fs::write(&local_path, local.to_string()).unwrap();

        let remote = serde_json::json!({
            "name": "piksel",
            "version": "1.2.0",
            "type": "app",
            "dependencies-sys": { "database": "1.0.3" }
        });
        let merged = merge_local_dependencies(&local_path, &remote.to_string()).unwrap();
        let value: serde_json::Value = serde_json::from_str(&merged).unwrap();

        assert_eq!(value["dependencies-sys"]["database"], "1.0.3");
        assert_eq!(value["dependencies-sys"]["chatllm"], "1.0.11");
        assert!(value.get("dependencies-design-plugins").is_none());
        assert!(value.get("dependencies-design-soft").is_none());
        assert_eq!(value["auto_update"], false);
    }
}
