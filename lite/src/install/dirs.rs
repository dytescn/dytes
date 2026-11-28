/// 跨平台获取默认的 Piksel 安装路径。
///
/// 优先用户级应用目录：Windows `%LOCALAPPDATA%\Programs\Piksel`、
/// macOS `~/Applications/Piksel`、Linux `~/.local/share/Piksel`；
/// 无法获取时回退到 `~/piksel`。
pub fn get_default_install_path() -> String {
    #[cfg(windows)]
    {
        if let Some(local) = dirs::data_local_dir() {
            return local
                .join("Programs")
                .join("Piksel")
                .to_string_lossy()
                .to_string();
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(home) = dirs::home_dir() {
            return home
                .join("Applications")
                .join("Piksel")
                .to_string_lossy()
                .to_string();
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(data) = dirs::data_local_dir() {
            return data.join("Piksel").to_string_lossy().to_string();
        }
    }

    dirs::home_dir()
        .map(|home| home.join("piksel").to_string_lossy().to_string())
        .unwrap_or_else(|| "piksel".to_string())
}
