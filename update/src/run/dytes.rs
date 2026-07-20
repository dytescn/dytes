use super::update_app;
use anyhow::Result;
use serde_json::Value;
use std::fs;
use crate::utils::version::check_version;

pub async fn init_dytes() -> Result<()> {
    println!("init_dytes called");

    let content = fs::read_to_string("dytes.json")?;
    let json_data: Value = serde_json::from_str(&content)?;

    let app_name = json_data
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("piksel")
        .to_string();

    let app_version = json_data
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("1.0.0")
        .to_string();

    println!("📦 检查更新: {} v{}", app_name, app_version);
    let _ = update_app::update_app_info(app_name, app_version).await;
    println!("✅ 版本比对完成");

    // let _ = tokio::task::spawn_blocking(move || {
    //     let rt = tokio::runtime::Runtime::new().unwrap();
    //     rt.block_on(async {
    //         let _ = update_app::update_app_info(app_name, app_version).await;
    //         println!("本比对完成");
    //     });
    // }).await;

    Ok(())
}
