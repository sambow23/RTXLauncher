use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};
use std::env;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub manually_specified_install_path: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub console_enabled: bool,
    pub dxlevel: Option<u32>,
    pub load_workshop_addons: bool,
    pub disable_chromium: bool,
    pub developer_mode: bool,
    pub tools_mode: bool,
    pub custom_launch_options: Option<String>,
    // Linux-specific launch settings
    pub linux_proton_path: Option<String>,
    pub linux_steam_root_override: Option<String>,
    pub linux_enable_proton_log: bool,
    pub linux_selected_proton_label: Option<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            manually_specified_install_path: None,
            width: None,
            height: None,
            // Defaults: enable console and workshop addons by default
            console_enabled: true,
            dxlevel: None,
            load_workshop_addons: true,
            disable_chromium: false,
            developer_mode: false,
            tools_mode: false,
            custom_launch_options: None,
            linux_proton_path: None,
            linux_steam_root_override: None,
            linux_enable_proton_log: false,
            linux_selected_proton_label: None,
        }
    }
}

pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    pub fn new() -> Result<Self> {
        let exe_dir = env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .ok_or_else(|| anyhow::anyhow!("failed to resolve launcher directory"))?;
        fs::create_dir_all(&exe_dir)?;
        Ok(Self { path: exe_dir.join("settings.toml") })
    }

    pub fn load(&self) -> Result<AppSettings> {
        if !self.path.exists() {
            return Ok(AppSettings::default());
        }
        let text = fs::read_to_string(&self.path)?;
        let settings: AppSettings = toml::from_str(&text)?;
        Ok(settings)
    }

    pub fn save(&self, settings: &AppSettings) -> Result<()> {
        let text = toml::to_string_pretty(settings)?;
        fs::write(&self.path, text)?;
        Ok(())
    }
}


