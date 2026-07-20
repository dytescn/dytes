use crate::app::{InstallerApp, Page};
use eframe::egui;

pub fn installing(app: &mut InstallerApp, ui: &mut egui::Ui) {
    // 第一次进入页面
    if !app.downloading {
        app.downloading = true;

        let (tx, rx) = std::sync::mpsc::channel();

        app.receiver = Some(rx);

        std::thread::spawn(move || {
            let _ = crate::service::download::download_file(
                "https://example.com/piksel.zip",
                "./download/piksel.zip",
                tx,
            );
        });
    }

    // 读取下载进度
    if let Some(rx) = &app.receiver {
        while let Ok(msg) = rx.try_recv() {
            match msg {
                DownloadMessage::Progress(v) => {
                    app.progress = v;
                }

                DownloadMessage::Finished => {
                    app.progress = 1.0;
                    app.page = Page::Finished;
                }

                DownloadMessage::Error(err) => {
                    app.status = err;
                }
            }
        }
    }

    // UI显示
    ui.vertical_centered(|ui| {
        ui.heading("正在下载");

        ui.add_space(20.0);

        ui.add(
            egui::ProgressBar::new(app.progress)
                .desired_width(400.0)
                .show_percentage(),
        );

        ui.label(format!("{:.1} %", app.progress * 100.0));
    });
}
