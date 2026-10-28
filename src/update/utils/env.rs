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
    // 检查是否在开发环境（通过环境变量或 cargo 特征判断）
    #[cfg(debug_assertions)]
    {
        // 开发模式：使用当前工作目录
        std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .to_string_lossy()
            .to_string()
    }

    #[cfg(not(debug_assertions))]
    {
        // 生产模式：使用可执行文件所在目录
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
