use downloader::{Downloader, Download};
use std::path::Path;

pub async fn download_file(url: &str, output_dir: &str) -> bool {
    let mut downloader = match Downloader::builder()
        .download_folder(Path::new(output_dir))
        .build() {
            Ok(d) => d,
            Err(e) => {
                eprintln!("创建下载器失败: {}", e);
                return false;
            }
        };
    
    let download = Download::new(url);
    
    match downloader.download(&[download]) {
        Ok(results) => {
            if let Ok(status) = &results[0] {
                println!("下载成功: {:?}", status.file_name);
                true
            } else {
                eprintln!("下载失败: {}", results[0].as_ref().unwrap_err());
                false
            }
        }
        Err(e) => {
            eprintln!("下载执行失败: {}", e);
            false
        }
    }
}

