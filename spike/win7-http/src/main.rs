//! Win7 Lite spike（HTTP 依赖集）：验证 reqwest 0.12 + tokio 1.38 +
//! Windows TLS 后端能否在 1.77 下构建，并检查导入表。

#[tokio::main]
async fn main() {
    let client = reqwest::Client::builder()
        .build()
        .expect("构建 reqwest client 失败");
    std::hint::black_box(&client);

    // 只创建不发送，避免 CI 依赖外网。
    let _ = std::hint::black_box(tokio::runtime::Handle::current());
    println!("win7-http ok");
}
