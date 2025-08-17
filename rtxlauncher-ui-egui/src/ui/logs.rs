use eframe::egui;

pub fn render_logs_tab(app: &mut crate::app::LauncherApp, ui: &mut egui::Ui) {
	ui.heading("Logs");
	ui.separator();
	
	ui.horizontal(|ui| {
		if ui.small_button("Copy").clicked() {
			ui.output_mut(|o| o.copied_text = app.log.clone());
		}
		if ui.small_button("Clear").clicked() {
			app.log.clear();
		}
	});
	
	ui.separator();
	
	let available_height = ui.available_height();
	egui::ScrollArea::vertical()
		.stick_to_bottom(true)
		.auto_shrink([false, false])
		.max_height(available_height)
		.show(ui, |ui| {
			ui.set_min_height(available_height - 20.0); // Leave some padding
			ui.monospace(&app.log);
		});
}
