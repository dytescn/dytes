use super::types::DownloadTask;
use std::path::PathBuf;

/// Linux 需要下载的全部文件
pub static MAC_DOWNLOAD_LIST: &[DownloadTask] = &[
    DownloadTask {
        url: "https://example.com/linux/bin1".to_string(),
        save_name: "bin1".to_string(),
    },
    DownloadTask {
        url: "https://example.com/linux/lib.tar.gz".to_string(),
        save_name: "lib.tar.gz".to_string(),
    },
];
