pub mod utils;
pub mod run;
use crate::run::dytes;
use slint::ComponentHandle;
slint::include_modules!();
type ProgressCallback = Box<dyn Fn(f32, &str) + Send + Sync>;

#[tokio::main]
async fn main() -> Result<(), slint::PlatformError> {
    println!("启动更新程序...");
    if !utils::env::check_network().await {
        println!("网络不可用，退出程序");
        std::process::exit(1);
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
                            // 移除 set_show_update_button 调用
                        }
                        Err(e) => {
                            ui.set_status_text(format!("更新失败: {}", e).into());
                            // 移除 set_show_update_button 调用
                        }
                    }
                    ui.set_show_progress(false);
                    ui.set_progress_value(100.0);
                }
            });
        });
    });
    
    ui.on_restart_clicked(|| {
        println!("正在重启应用...");
        std::process::exit(0);
    });
    
    ui.run()
}