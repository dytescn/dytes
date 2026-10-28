use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};

pub async fn check_network() -> bool {
    let address = "223.5.5.5:53";
    matches!(
        timeout(Duration::from_secs(3), TcpStream::connect(address)).await,
        Ok(Ok(_))
    )
}

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
        // 兼容旧 update.exe：清单在可执行文件同目录。
        std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .to_string_lossy()
            .to_string()
    }
}

/// 获取临时目录
pub fn get_temp_dir() -> String {
    std::env::temp_dir()
        .join("piksel_update")
        .to_string_lossy()
        .to_string()
}

/// 获取当前可执行文件路径
pub fn get_exe_path() -> String {
    std::env::current_exe()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .to_string_lossy()
        .to_string()
}
