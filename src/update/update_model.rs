use anyhow::{Context, Result};
use std::fs;

use crate::utils::checksum::verify_download;
use crate::utils::dytes::DytesConfig;
use crate::update::env::get_app_dir;
use crate::update::loader::{download_file, fetch_text};
use crate::update::lock::{get_package_version_from_lock, update_package_in_lock};
use crate::utils::version::{check_version, clean_declared as parse_version};


/// 安装模型包到 models/<name>/。
/// 模型视为 chatllm 等组件的强制关联文件，版本独立于代码包。
pub async fn update_model_info(name: String, declared_version: String) -> Result<()> {
    println!("更新模型: {} (声明版本 {})", name, declared_version);

    let s3_path = format!("models/{}/dytes.json", name);
    println!("  从 S3 获取: {}", s3_path);
    let remote_text = fetch_text(&s3_path)
        .await
        .with_context(|| format!("获取 {} 失败", s3_path))?;
    let remote_info: DytesConfig =
        serde_json::from_str(&remote_text).with_context(|| format!("解析 {} 失败", s3_path))?;

    // 仅清单模型：没有产物，只保存清单并登记版本。
    if remote_info.exports.is_empty() && remote_info.platform_exports.is_empty() {
        let app_dir = get_app_dir();
        let model_dir = format!("{}/models/{}", app_dir.trim_end_matches('/'), name);
        fs::create_dir_all(&model_dir)?;
        fs::write(format!("{}/dytes.json", model_dir), &remote_text)?;
        update_package_in_lock(
            remote_info.uuid.as_deref(),
            &name,
            &remote_info.version,
            Some(&declared_version),
            "model",
            Vec::new(),
        )?;
        println!("  仅清单模型已安装: {} v{}", name, remote_info.version);
        return Ok(());
    }

    let platform = crate::utils::platform::current_platform();
    let Some((exports, platform_tag)) =
        crate::utils::platform::select_exports(&remote_info, &platform)
    else {
        anyhow::bail!("模型 {} 未发布平台 {}", name, platform);
    };
    let base_path = crate::utils::platform::remote_base(
        &format!("models/{}", name),
        &remote_info.version,
        platform_tag,
    );

    println!("  远程版本: {}", remote_info.version);
    println!("  平台: {}", platform);

    if let Ok(Some(lock_version)) = get_package_version_from_lock(&name) {
        if let (Some(local), Some(remote)) = (
            parse_version(&lock_version),
            parse_version(&remote_info.version),
        ) {
            if !check_version(&local, &remote) {
                println!("  已是最新版本 (v{})，跳过下载", local);
                return Ok(());
            }
        }
    }

    let app_dir = get_app_dir();
    let model_dir = format!("{}/models/{}", app_dir.trim_end_matches('/'), name);
    fs::create_dir_all(&model_dir)?;
    println!("  安装目录: {}", model_dir);

    let mut loader = Vec::new();
    for (source_path, dest_filename) in exports {
        if dest_filename.contains('/') || dest_filename.contains(std::path::MAIN_SEPARATOR) {
            anyhow::bail!("不支持的导出路径（只允许文件名）: {}", dest_filename);
        }
        let s3_file = format!("{}{}", base_path, dest_filename);
        println!("  下载: {} -> {}/{}", s3_file, model_dir, dest_filename);
        if !download_file(&s3_file, &model_dir).await {
            anyhow::bail!("下载模型 {}/{} 失败", name, dest_filename);
        }
        verify_download(
            std::path::Path::new(&format!("{}/{}", model_dir, dest_filename)),
            dest_filename,
            &remote_info.checksums,
        )?;
        loader.push(vec![source_path.clone(), dest_filename.clone()]);
    }

    update_package_in_lock(
        remote_info.uuid.as_deref(),
        &name,
        &remote_info.version,
        Some(&declared_version),
        "model",
        loader,
    )?;

    println!("  模型更新完成: {} v{}", name, remote_info.version);
    Ok(())
}
