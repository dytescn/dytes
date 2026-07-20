use super::types::DownloadTask;
use std::path::PathBuf;

/// Windows 需要下载的全部文件
pub static WIN_DOWNLOAD_LIST: &[DownloadTask] = &[
    DownloadTask {
        url: "https://example.com/win/bin1.exe".to_string(),
        save_name: "bin1.exe".to_string(),
    },
    DownloadTask {
        url: "https://example.com/win/lib.zip".to_string(),
        save_name: "lib.zip".to_string(),
    },
];

