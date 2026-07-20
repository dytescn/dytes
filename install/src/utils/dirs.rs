/// 跨平台获取默认piksel安装路径
pub fn get_default_install_path() -> String {
    #[cfg(windows)]
    {
        let d_drive_path = std::path::Path::new(r#"D:\piksel"#);
        // 检查D盘根目录是否可访问
        if std::fs::try_exists(d_drive_path.parent().unwrap()).unwrap_or(false) {
            return d_drive_path.to_string_lossy().to_string();
        } else {
            // D盘不存在，回退到用户目录
            let home = dirs::home_dir().expect("无法获取用户主目录");
            return home.join("piksel").to_string_lossy().to_string();
        }
    }

    #[cfg(not(windows))]
    {
        // mac / linux
        let home = dirs::home_dir().expect("无法获取用户主目录");
        home.join("piksel").to_string_lossy().to_string()
    }
}

