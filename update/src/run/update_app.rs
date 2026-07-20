use std::fs;
use std::collections::HashMap;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value; 
use crate::utils::version::check_version;

#[derive(Debug, Deserialize, Serialize)]
pub struct PackagesInfo {
    pub packages: HashMap<String, String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PackageDetail {
    pub exports: Vec<Vec<String>>,  // 如: [["./executable", "./"], ["./web/index.html", "./web/"]]
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PackageLock {
    pub name: String,
    pub version: String,
    #[serde(rename = "type")]
    pub pkg_type: String,
    pub loader: Vec<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DytesLock {
    pub package: Vec<PackageLock>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RemoteInfo {
    pub packages: HashMap<String, String>, // or whatever type your versions are
}

pub async fn update_app_info(name:String,version:String) -> Result<()> {
    println!("开始检查应用更新...");
    let url = "https://packages.funxdata.com/app/packages.json";
    let response = reqwest::get(url).await?;
    if !response.status().is_success() {
        anyhow::bail!("下载 packages.json 失败: HTTP {}", response.status());
    }
    let dytes_app_remote_json_text = response.text().await?;
    let dytes_app_remote_json: RemoteInfo = serde_json::from_str(&dytes_app_remote_json_text)?;
    let remote_version = dytes_app_remote_json.packages.get(&name).unwrap_or(&version);
    println!("远程应用版本: {:?}", remote_version);
    if !check_version(&version, remote_version) {
        println!("当前已是最新版本");
        return Ok(());
    }
    // 执行更新逻辑
    println!("开始准备更新...");
        
    // 构建 dytes.json 的 URL
    let detail_url = format!("https://packages.funxdata.com/app/{}/dytes.json", name);
    let detail_response = reqwest::get(&detail_url).await?;
    if !detail_response.status().is_success() {
        eprintln!("获取 {}/dytes.json 失败，跳过", name);
        return Ok(());
    }
    let remote_app_info_text = detail_response.text().await?;
    let remote_app_info: PackageDetail = serde_json::from_str(&remote_app_info_text)?;
    // 整理exports数据
    // 将整理好的exports数据写入dytes.lock
    Ok(())
}
