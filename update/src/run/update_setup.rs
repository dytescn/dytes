use anyhow::{Result, Context as AnyhowContext};
use std::fs;
use std::path::Path;
use rquickjs::{Context, Runtime};

/// 执行 init 脚本
pub fn execute_init_script(script_path: &str) -> Result<()> {
    println!("  执行初始化脚本: {}", script_path);
    // 读取脚本内容
    let script_content = fs::read_to_string(script_path)
        .with_context(|| format!("读取脚本文件失败: {}", script_path))?;
    
    // 创建 QuickJS 运行时
    let runtime = Runtime::new()?;
    let context = Context::full(&runtime)?;
    
    // 执行脚本
    context.with(|ctx| {
        match ctx.eval::<rquickjs::Value, _>(script_content.as_str()) {
            Ok(result) => {
                // 尝试将结果转换为字符串并打印
                if let Some(s) = result.as_string() {
                    if let Ok(s) = s.to_string() {
                        if !s.is_empty() {
                            println!("  脚本输出: {}", s);
                        }
                    }
                }
                println!("  init 脚本执行成功");
                Ok::<(), anyhow::Error>(())
            }
            Err(e) => {
                eprintln!("  执行 init 脚本失败: {}", e);
                Ok::<(), anyhow::Error>(())
            }
        }
    })?;
    
    Ok(())
}

/// 执行 Web 资源的 init 脚本
/// 
/// # 参数
/// - `web_dir`: Web 资源目录，例如 `./web/init/`
/// - `init_scripts`: init 脚本文件名列表，例如 `["./setup.js"]`
/// 
/// # 说明
/// 该函数只执行本地已下载的脚本，不进行任何远程调用
pub fn setup_webs_init(web_dir: &str, init_scripts: &[String]) -> Result<()> {
    if init_scripts.is_empty() {
        return Ok(());
    }
    println!("  检测到 {} 个 init 脚本，开始执行...", init_scripts.len());    
    for script in init_scripts {
        // 拼接完整的脚本路径：web_dir + script
        let script_path = Path::new(web_dir).join(script);
        
        if script_path.exists() {
            if let Err(e) = execute_init_script(script_path.to_str().unwrap_or(script)) {
                eprintln!("  执行 init 脚本 {} 失败: {}", script, e);
            }
        } else {
            eprintln!("  init 脚本不存在: {}", script_path.display());
        }
    }
    
    Ok(())
}