use anyhow::{Result, Context as AnyhowContext};
use std::fs;
use std::path::Path;
use std::process::Command;
use rquickjs::{Context, Runtime, Function, Object};
use std::thread;
use std::time::Duration;

const PID_FILE: &str = "./run.pid";

/// 检测 cyctron.exe 是否运行
pub fn is_cyctron_running() -> bool {
    if !std::path::Path::new(PID_FILE).exists() {
        return false;
    }
    if let Ok(content) = fs::read_to_string(PID_FILE) {
        if let Ok(pid) = content.trim().parse::<u32>() {
            return is_process_alive(pid);
        }
    }
    false
}

/// 检测进程是否存活
fn is_process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    unsafe { libc::kill(pid as i32, 0) == 0 }
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
        match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(handle) => { let _ = CloseHandle(handle); true }
            Err(_) => false,
        }
    }
}

/// 停止 cyctron.exe
pub fn stop_cyctron() -> Result<()> {
    if !is_cyctron_running() {
        println!("  cyctron.exe 未运行");
        return Ok(());
    }

    let content = fs::read_to_string(PID_FILE)?;
    let pid: u32 = content.trim().parse()?;
    
    #[cfg(windows)]
    {
        let output = Command::new("taskkill")
            .args(&["/PID", &pid.to_string(), "/F"])
            .output()?;
        
        if output.status.success() {
            println!("  cyctron.exe (PID: {}) 已停止", pid);
            let _ = fs::remove_file(PID_FILE);
            Ok(())
        } else {
            let err_msg = String::from_utf8_lossy(&output.stderr);
            Err(anyhow::anyhow!("停止 cyctron.exe 失败: {}", err_msg))
        }
    }
    
    #[cfg(unix)]
    {
        unsafe {
            if libc::kill(pid as i32, libc::SIGTERM) == 0 {
                println!("  cyctron.exe (PID: {}) 已停止", pid);
                let _ = fs::remove_file(PID_FILE);
                Ok(())
            } else {
                Err(anyhow::anyhow!("停止 cyctron.exe 失败"))
            }
        }
    }
}

/// 启动 cyctron.exe
pub fn start_cyctron() -> Result<()> {
    if is_cyctron_running() {
        println!("  cyctron.exe 已在运行");
        return Ok(());
    }
    
    let exe = if cfg!(windows) { "./cyctron.exe" } else { "./cyctron" };
    println!("  启动: {}", exe);
    
    let child = Command::new(exe)
        .arg("start")
        .spawn()
        .context("启动 cyctron.exe 失败")?;
    
    thread::sleep(Duration::from_millis(500));
    
    #[cfg(windows)]
    let pid = child.id();
    
    #[cfg(unix)]
    let pid = child.id().ok_or_else(|| anyhow::anyhow!("无法获取进程 PID"))?;
    
    fs::write(PID_FILE, pid.to_string())?;
    println!("  cyctron.exe (PID: {}) 已启动", pid);
    
    Ok(())
}

/// 执行单个 init 脚本（升级版：支持 await fetch 转换）
fn execute_init_script(script_path: &str) -> Result<()> {
    println!("  执行初始化脚本: {}", script_path);
    let script_content = fs::read_to_string(script_path)
        .with_context(|| format!("读取脚本文件失败: {}", script_path))?;

    // 1. 转换：await fetch( -> rustFetch(
    let mut transformed = script_content.replace("await fetch(", "rustFetch(");

    // 2. 自动提取变量名并添加返回语句（如果存在 const 变量 = rustFetch(...)）
    if let Some(start) = transformed.find("const ") {
        let after_const = &transformed[start + 6..];
        if let Some(eq_pos) = after_const.find('=') {
            let var_name = after_const[..eq_pos].trim();
            if !transformed.trim_end().ends_with(&format!("{};", var_name)) {
                transformed = format!("{}\n{};", transformed.trim_end(), var_name);
            }
        }
    }

    println!("  转换后脚本:\n{}", transformed);

    let runtime = Runtime::new()?;
    let context = Context::full(&runtime)?;

    context.with(|ctx| -> Result<(), anyhow::Error> {
        // 注册 rustFetch 同步函数
        let rust_fetch = Function::new(ctx.clone(), |url: String, options: Object| -> String {
            let method = options
                .get::<_, String>("method")
                .unwrap_or_else(|_| "GET".to_string());
            let headers = options.get::<_, Object>("headers").ok();
            let body = options.get::<_, String>("body").ok();

            // 使用 blocking client，禁用代理
            let client = reqwest::blocking::Client::builder()
                .no_proxy()
                .build()
                .unwrap();

            let method_parsed = match method.to_uppercase().as_str() {
                "GET" => reqwest::Method::GET,
                "POST" => reqwest::Method::POST,
                "PUT" => reqwest::Method::PUT,
                "DELETE" => reqwest::Method::DELETE,
                "PATCH" => reqwest::Method::PATCH,
                _ => reqwest::Method::GET,
            };

            let mut request = client.request(method_parsed, &url);

            if let Some(headers_obj) = headers {
                let keys: Vec<String> = headers_obj
                    .keys::<String>()
                    .filter_map(Result::ok)
                    .collect();
                for key in keys {
                    if let Ok(value) = headers_obj.get::<_, String>(&key) {
                        request = request.header(&key, value);
                    }
                }
            }

            if let Some(body_str) = body {
                request = request.body(body_str);
            }

            match request.send() {
                Ok(response) => match response.text() {
                    Ok(text) => text,
                    Err(e) => format!("读取响应失败: {}", e),
                },
                Err(e) => format!("请求失败: {}", e),
            }
        })?;
        ctx.globals().set("rustFetch", rust_fetch)?;

        // 执行转换后的脚本
        match ctx.eval::<rquickjs::Value, _>(transformed.as_str()) {
            Ok(result) => {
                if let Some(s) = result.as_string() {
                    if let Ok(s) = s.to_string() {
                        if !s.is_empty() {
                            println!("  脚本输出: {}", s);
                        }
                    }
                }
                println!("  init 脚本执行成功");
                Ok(())
            }
            Err(e) => {
                eprintln!("  执行 init 脚本失败: {}", e);
                Err(anyhow::anyhow!("QuickJS 执行失败: {}", e))
            }
        }
    })?;

    Ok(())
}

/// 主入口：执行 Web 资源的 init 脚本
pub fn setup_webs_init(web_dir: &str, init_scripts: &[String]) -> Result<()> {
    if init_scripts.is_empty() {
        println!("  没有 init 脚本需要执行");
        return Ok(());
    }
    
    println!("  检测到 {} 个 init 脚本，开始执行...", init_scripts.len());
    
    // 启动 cyctron
    if !is_cyctron_running() {
        println!("  cyctron.exe 未运行，先启动...");
        start_cyctron()?;
    } else {
        println!("  cyctron.exe 已在运行");
    }
    
    // 执行所有脚本
    let mut has_error = false;
    for script in init_scripts {
        let clean_script = script.trim_start_matches("./");
        let script_path = Path::new(web_dir).join(clean_script);
        
        if script_path.exists() {
            if let Err(e) = execute_init_script(script_path.to_str().unwrap_or(script)) {
                eprintln!("  执行 init 脚本 {} 失败: {}", script, e);
                has_error = true;
            }
        } else {
            eprintln!("  init 脚本不存在: {}", script_path.display());
            has_error = true;
        }
    }
    
    // 无论成功还是失败，都关闭 cyctron
    println!("  关闭 cyctron.exe...");
    if let Err(e) = stop_cyctron() {
        eprintln!("  关闭 cyctron.exe 失败: {}", e);
    }
    
    if has_error {
        println!("  部分脚本执行失败");
        Err(anyhow::anyhow!("部分脚本执行失败"))
    } else {
        println!("  所有 init 脚本执行完成");
        Ok(())
    }
}