use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};

pub async fn check_network() -> bool {
    // 尝试连接 Cloudflare 的 DNS over HTTPS 服务器
    let address = "223.5.5.5:53";
    match timeout(Duration::from_secs(3), TcpStream::connect(address)).await {
        Ok(Ok(_stream)) => true, // 连接成功
        _ => false, // 连接失败或超时
    }
}
