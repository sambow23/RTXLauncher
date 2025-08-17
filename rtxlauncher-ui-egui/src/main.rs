#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod ui;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    rtxlauncher_core::init_logging();
    let _store = rtxlauncher_core::SettingsStore::new()?;
	let mut native_options = eframe::NativeOptions::default();
	// Configure window min and initial size using the viewport builder (eframe 0.29)
	native_options.viewport = native_options
		.viewport
		.with_inner_size([874.0, 500.0])
		.with_min_inner_size([874.0, 500.0])
		.with_resizable(true);
	
	eframe::run_native(
		"RTXLauncher-rs",
		native_options,
        Box::new(|_cc| Ok(Box::new(app::LauncherApp::default()))),
	).unwrap();
	Ok(())
}


