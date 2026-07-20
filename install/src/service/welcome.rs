use crate::app::InstallerApp;
use eframe::egui;
fn welcome(app: &mut InstallerApp, ui: &mut egui::Ui) {
    ui.vertical_centered(|ui| {
        ui.heading("欢迎使用安装器");

        ui.add_space(20.0);

        if ui.button("开始安装").clicked() {
            app.page = Page::Installing;
        }
    });
}
