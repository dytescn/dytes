use reqwest::blocking::Response;
use std::io::{Read, Write};
use std::path::PathBuf;
use super::win::DownloadTask;

pub fn download_file_with_progress<F>(
    url: &str,
    save_path: PathBuf,
    mut progress_cb: F,
) -> reqwest::Result<()>
where
    F: FnMut(u64, u64),
{
    let mut resp: Response = reqwest::blocking::get(url)?;
    let total_size = resp.content_length().unwrap_or(0);
    let mut file = std::fs::File::create(&save_path)?;

    let mut downloaded: u64 = 0;
    let mut buf = [0u8; 8192];

    loop {
        let n = resp.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        downloaded += n as u64;
        progress_cb(downloaded, total_size);
    }

    Ok(())
}



/// 根据当前操作系统获取对应下载任务列表
pub fn get_os_download_list() -> &'static [DownloadTask] {
    #[cfg(windows)]
    return super::WIN_DOWNLOAD_LIST;

    #[cfg(target_os = "linux")]
    return super::LINUX_DOWNLOAD_LIST;

    #[cfg(target_os = "macos")]
    return super::MACOS_DOWNLOAD_LIST;
}

