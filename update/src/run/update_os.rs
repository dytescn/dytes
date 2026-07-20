use std::collections::HashMap;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use crate::utils::version::check_version;

#[derive(Debug, Deserialize, Serialize)]
pub struct PackagesInfo {
    pub packages: HashMap<String, String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PackageDetail {
    pub exports: Vec<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RemoteInfo {
    pub packages: HashMap<String, String>,
}

/// 更新操作系统
pub async fn update_os_info(name: String, version: String) -> Result<()> {
    println!("🖥️ 开始检查操作系统更新: {} v{}", name, version);
    
    #[cfg(target_os = "windows")]
    {
        println!("  🪟 Windows 系统更新检查...");
        let url = "https://packages.funxdata.com/os/windows/packages.json";
        let response = reqwest::get(url).await?;
        if !response.status().is_success() {
            anyhow::bail!("下载 packages.json 失败: HTTP {}", response.status());
        }
        let remote_json_text = response.text().await?;
        let remote_json: RemoteInfo = serde_json::from_str(&remote_json_text)?;
        let remote_version = remote_json.packages.get(&name).unwrap_or(&version);
        println!("  远程系统版本: {:?}", remote_version);
        
        if !check_version(&version, remote_version) {
            println!("  ✅ 当前已是最新版本");
            return Ok(());
        }
        
        println!("  🔄 发现新的系统更新，开始准备更新...");
        // Windows 系统更新逻辑
        // 可以调用 Windows Update API
        println!("  ℹ️ Windows 系统更新需要通过 Windows Update 进行");
    }
    
    #[cfg(target_os = "macos")]
    {
        println!("  🍎 macOS 系统更新检查...");
        let url = "https://packages.funxdata.com/os/macos/packages.json";
        let response = reqwest::get(url).await?;
        if !response.status().is_success() {
            anyhow::bail!("下载 packages.json 失败: HTTP {}", response.status());
        }
        let remote_json_text = response.text().await?;
        let remote_json: RemoteInfo = serde_json::from_str(&remote_json_text)?;
        let remote_version = remote_json.packages.get(&name).unwrap_or(&version);
        println!("  远程系统版本: {:?}", remote_version);
        
        if !check_version(&version, remote_version) {
            println!("  ✅ 当前已是最新版本");
            return Ok(());
        }
        
        println!("  🔄 发现新的系统更新，开始准备更新...");
        // macOS 系统更新逻辑
        println!("  ℹ️ macOS 系统更新需要通过 Software Update 进行");
    }
    
    #[cfg(target_os = "linux")]
    {
        println!("  🐧 Linux 系统更新检查...");
        let url = "https://packages.funxdata.com/os/linux/packages.json";
        let response = reqwest::get(url).await?;
        if !response.status().is_success() {
            anyhow::bail!("下载 packages.json 失败: HTTP {}", response.status());
        }
        let remote_json_text = response.text().await?;
        let remote_json: RemoteInfo = serde_json::from_str(&remote_json_text)?;
        let remote_version = remote_json.packages.get(&name).unwrap_or(&version);
        println!("  远程系统版本: {:?}", remote_version);
        
        if !check_version(&version, remote_version) {
            println!("  ✅ 当前已是最新版本");
            return Ok(());
        }
        
        println!("  🔄 发现新的系统更新，开始准备更新...");
        // Linux 系统更新逻辑
        println!("  ℹ️ Linux 系统更新需要通过包管理器进行");
        println!("     apt: sudo apt update && sudo apt upgrade");
        println!("     yum: sudo yum update");
        println!("     pacman: sudo pacman -Syu");
    }
    
    Ok(())
}