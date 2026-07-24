mod utils;
mod run;

use std::env;
use std::path::Path;

fn main() {
    // 获取当前目录
    let current_dir = env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
    let json_path = current_dir.join("dytes.json");
    
    // 检查 dytes.json 是否存在
    if !json_path.exists() {
        eprintln!("错误: 找不到 dytes.json 文件");
        std::process::exit(1);
    }
    
    println!("========== Publish 工具 ==========");
    println!("配置文件: {}", json_path.display());
    println!();
    
    // 执行发布
    if let Err(e) = run::dytes::publish(&json_path) {
        eprintln!("发布失败: {}", e);
        std::process::exit(1);
    }
    
    println!("\n发布完成！");
}