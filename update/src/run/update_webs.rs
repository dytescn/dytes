use anyhow::Result;

pub async fn update_webs_info(name: String, version: String) -> Result<()> {
    println!("🌐 更新 Web 资源信息: {} v{}", name, version);
    
    // 构建 dytes.json 的 URL
    let detail_url = format!("https://packages.funxdata.com/webs/{}/dytes.json", name);
    let detail_response = reqwest::get(&detail_url).await?;
    if !detail_response.status().is_success() {
        eprintln!("获取 {}/dytes.json 失败，跳过", name);
        return Ok(());
    }
    let _remote_app_info_text = detail_response.text().await?;
    // let remote_app_info: PackageDetail = serde_json::from_str(&remote_app_info_text)?;
    // println!("{:?}", remote_app_info);
    
    Ok(())
}