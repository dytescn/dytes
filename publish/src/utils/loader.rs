use std::fs::File;
use std::io::Read;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client;
use aws_sdk_s3::config::{Region, Config};
use aws_config::BehaviorVersion;
use aws_credential_types::Credentials;

const ENDPOINT_URL: &str = "https://s3.api.upyun.com";
const ACCESS_KEY: &str = "195cc6d12c2d45389a5e52e5138f7250";
const SECRET_KEY: &str = "7ab145ae97310ca9f65db3dff07beac2";

/// 上传文件到又拍云
pub fn upload_file(file_path: &str, bucket_name: &str, object_name: &str) -> bool {
    println!("   上传文件: {}", file_path);
    println!("   目标: {}/{}", bucket_name, object_name);
    
    // 读取文件内容
    let mut file_content = Vec::new();
    if let Err(e) = File::open(file_path).and_then(|mut f| f.read_to_end(&mut file_content)) {
        eprintln!("  读取文件失败: {}", e);
        return false;
    }
    
    println!("  文件大小: {} bytes", file_content.len());
    
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    let result = rt.block_on(async {
        let credentials = Credentials::new(ACCESS_KEY, SECRET_KEY, None, None, "custom");
        
        // 使用路径风格（path-style）
        let config = aws_config::defaults(BehaviorVersion::latest())
            .endpoint_url(ENDPOINT_URL)
            .region(Region::new("us-east-1"))
            .credentials_provider(credentials)
            .load()
            .await;
        
        // 强制使用路径风格
        let config = Config::from(&config)
            .to_builder()
            .force_path_style(true)
            .build();
        
        let client = Client::from_conf(config);
        
        client
            .put_object()
            .bucket(bucket_name)
            .key(object_name)
            .body(ByteStream::from(file_content))
            .acl(aws_sdk_s3::types::ObjectCannedAcl::PublicRead)
            .send()
            .await
    });
    
    match result {
        Ok(_) => {
            println!("  文件上传成功: {}", object_name);
            true
        }
        Err(e) => {
            eprintln!("  文件上传失败: {}", e);
            false
        }
    }
}

/// 同步下载文本
pub fn download_text_sync(url: &str) -> Result<String, reqwest::Error> {
    use reqwest::blocking::Client;
    let response = Client::new().get(url).send()?;
    response.text()
}