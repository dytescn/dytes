pub mod utils;
pub mod run;

use crate::run::dytes;
use slint::ComponentHandle;
use std::process;
use std::path::Path;

slint::include_modules!();

type ProgressCallback = Box<dyn Fn(f32, &str) + Send + Sync>;

#[tokio::main]
async fn main() -> Result<(), slint::PlatformError> {
    println!("启动更新程序...");
    
    if !utils::env::check_network().await {
        println!("网络不可用，退出程序");
        process::exit(1);
    }
    
    // 检查是否有更新
    let has_updates = match dytes::check_updates().await {
        Ok(b) => b,
        Err(e) => {
            println!("检查更新失败: {}", e);
            process::exit(1);
        }
    };
    
    if !has_updates {
        println!("所有依赖已是最新版本，无需更新");
        println!("程序将在 2 秒后自动退出...");
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        process::exit(0);
    }
    
    let ui = UpdatePage::new()?;
    let ui_handle = ui.as_weak();
    
    ui.on_update_clicked(move || {
        let ui = ui_handle.unwrap();
        let ui_weak = ui.as_weak();
        
        ui.set_show_progress(true);
        ui.set_status_text("开始检查更新...".into());
        ui.set_progress_value(0.0);
        
        let ui_callback = ui_weak.clone();
        let ui_final = ui_weak.clone();
        
        tokio::spawn(async move {
            let progress_callback: ProgressCallback = Box::new(move |progress: f32, status: &str| {
                let status_str = status.to_string();
                let ui = ui_callback.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = ui.upgrade() {
                        ui.set_progress_value(progress);
                        ui.set_status_text(status_str.into());
                    }
                });
                println!("进度: {:.1}% - {}", progress, status);
            });
            
            let result = dytes::init_dytes_with_progress(Some(progress_callback)).await;
            
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = ui_final.upgrade() {
                    match result {
                        Ok(_) => {
                            ui.set_status_text("更新完成!".into());
                            ui.set_show_restart(true);
                        }
                        Err(e) => {
                            ui.set_status_text(format!("更新失败: {}", e).into());
                        }
                    }
                    ui.set_show_progress(false);
                    ui.set_progress_value(100.0);
                }
            });
        });
    });
    
    ui.on_restart_clicked(move || {
        println!("正在重启应用...");
        
        // 从 dytes.json 读取应用名称
        let app_name = match dytes::get_app_name() {
            Ok(name) => name,
            Err(e) => {
                println!("读取应用名称失败: {}", e);
                process::exit(1);
            }
        };
        
        // 构建应用文件名
        let app_exe = if cfg!(target_os = "windows") {
            format!("{}.exe", app_name)
        } else {
            app_name.clone()
        };
        
        // 获取当前可执行文件所在目录（即 update.exe 所在目录）
        let current_exe = match std::env::current_exe() {
            Ok(path) => path,
            Err(e) => {
                println!("获取当前路径失败: {}", e);
                process::exit(1);
            }
        };
        
        // 应用就在 update.exe 同目录下
        let app_dir = current_exe.parent().unwrap_or(Path::new("."));
        let app_path = app_dir.join(&app_exe);
        
        // 检查应用是否存在
        if !app_path.exists() {
            println!("应用不存在: {}", app_path.display());
            println!("请确保 {} 已更新或存在于正确位置", app_exe);
            process::exit(1);
        }
        
        println!("启动应用: {}", app_path.display());
        
        // 启动应用
        match process::Command::new(&app_path).spawn() {
            Ok(_) => {
                println!("应用已启动");
                process::exit(0);
            }
            Err(e) => {
                println!("启动应用失败: {}", e);
                process::exit(1);
            }
        }
    });
    
    ui.run()
}