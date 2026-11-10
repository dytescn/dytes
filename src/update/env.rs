/// 获取应用目录
/// 开发时使用当前工作目录，生产时使用可执行文件所在目录
pub fn get_app_dir() -> String {
    if let Ok(dir) = std::env::var("DYTES_APP_DIR") {
        let dir = dir.trim();
        if !dir.is_empty() {
            return dir.to_string();
        }
    }

    let cwd = std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .to_string_lossy()
        .to_string();

    // dytes 作为全局命令时，当前目录里的 dytes.json 才是应用清单。
    if std::path::Path::new(&cwd).join("dytes.json").exists() {
        return cwd;
    }

    #[cfg(debug_assertions)]
    {
        cwd
    }

    #[cfg(not(debug_assertions))]
    {
        // 兼容旧安装目录：清单可能位于可执行文件同目录。
        std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .to_string_lossy()
            .to_string()
    }
}
