pub mod utils;
pub mod run;
use crate::run::dytes;
use slint::ComponentHandle;
use std::time::Duration;
use tokio::time::sleep;
slint::include_modules!();

#[tokio::main]
async fn main() -> Result<(), slint::PlatformError> {
    println!("正在检查网络环境...");
    if !utils::env::check_network().await {
        std::process::exit(1);
    }
    // 检查是否需要更新
    let _ = dytes::init_dytes().await;
    // 启动UI更新
    //
    // 下载更新
    //
    // 初始化相关数据
    //
    // 更新完成
    let ui = UpdatePage::new()?;
    let ui_handle = ui.as_weak();
    
    // 注意：回调名称使用下划线，自动转换为 set_on_update_clicked
    ui.on_update_clicked(move || {
        let ui = ui_handle.unwrap();
        let ui_clone = ui.as_weak();
        
        // 属性设置也使用下划线
        ui.set_show_progress(true);
        ui.set_status_text("更新中...".into());
        ui.set_progress_value(0.0);
        
        tokio::spawn(async move {
            for progress in (0..=100).step_by(5) {
                sleep(Duration::from_millis(150)).await;
                
                if let Some(ui) = ui_clone.upgrade() {
                    ui.set_progress_value(progress as f32);
                    
                    if progress < 30 {
                        ui.set_status_text("下载更新包...".into());
                    } else if progress < 60 {
                        ui.set_status_text("安装更新...".into());
                    } else if progress < 90 {
                        ui.set_status_text("配置更新...".into());
                    } else {
                        ui.set_status_text("即将完成...".into());
                    }
                }
            }
            
            if let Some(ui) = ui_clone.upgrade() {
                ui.set_status_text("更新完成!".into());
                ui.set_show_progress(false);
                ui.set_show_restart(true);
                ui.set_progress_value(100.0);
            }
        });
    });
    
    ui.on_restart_clicked(|| {
        println!("正在重启应用...");
        std::process::exit(0);
    });
    
    ui.run()
}
