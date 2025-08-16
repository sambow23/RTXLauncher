#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod ui;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    rtxlauncher_core::init_logging();
    let _store = rtxlauncher_core::SettingsStore::new()?;
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "RTXLauncher (Rust)",
        native_options,
        Box::new(|_cc| Ok(Box::new(app::LauncherApp::default()))),
    ).unwrap();
    Ok(())
}


