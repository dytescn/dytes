use super::win::DownloadTask;

pub static LINUX_DOWNLOAD_LIST: &[DownloadTask] = &[
    DownloadTask {
        url: "https://example.com/linux/app".to_string(),
        save_name: "app".to_string(),
    },
    DownloadTask {
        url: "https://example.com/linux/runtime.tar.gz".to_string(),
        save_name: "runtime.tar.gz".to_string(),
    },
];



/// Windows/Linux/macOS 主程序名称
pub const EXE_NAMES: &[&str] = &["Piksel.exe", "piksel", "Piksel.app"];

/// 前端入口文件
pub const JS_FILES: &[&str] = &["index.js", "script.js", "main.js"];

/// CSS 文件
pub const CSS_FILES: &[&str] = &["style.css"];
