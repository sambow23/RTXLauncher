use crate::settings::AppSettings;
use std::path::PathBuf;
use std::process::Command;

fn split_args_quoted(src: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut quote_char: char = '\0';
    let mut escape = false;
    for ch in src.chars() {
        if escape {
            cur.push(ch);
            escape = false;
            continue;
        }
        match ch {
            '\\' => { escape = true; }
            '"' | '\'' => {
                if in_quotes {
                    if ch == quote_char { in_quotes = false; } else { cur.push(ch); }
                } else {
                    in_quotes = true; quote_char = ch;
                }
            }
            c if c.is_whitespace() && !in_quotes => {
                if !cur.is_empty() { out.push(cur.clone()); cur.clear(); }
            }
            _ => cur.push(ch),
        }
    }
    if !cur.is_empty() { out.push(cur); }
    out
}

pub fn build_launch_args(settings: &AppSettings) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    if settings.console_enabled { args.push("-console".into()); }
    // Always enforce DX level 90 as requested (two separate argv entries)
    args.push("-dxlevel".into());
    args.push("90".into());
    // D3D9Ex disable and windowing flags (each token separately)
    args.push("+mat_disable_d3d9ex".into()); args.push("1".into());
    args.push("-nod3d9ex".into());
    args.push("-windowed".into());
    args.push("-noborder".into());
    if let (Some(w), Some(h)) = (settings.width, settings.height) {
        if w > 0 && h > 0 {
            args.push("-w".into()); args.push(w.to_string());
            args.push("-h".into()); args.push(h.to_string());
        }
    }
    if !settings.load_workshop_addons { args.push("-noworkshop".into()); }
    if settings.disable_chromium { args.push("-nochromium".into()); }
    if settings.developer_mode { args.push("-dev".into()); }
    if settings.tools_mode { args.push("-tools".into()); }
    if let Some(custom) = &settings.custom_launch_options {
        let extra = split_args_quoted(custom);
        args.extend(extra);
    }
    args
}

#[cfg(windows)]
pub fn launch_game(exe_path: PathBuf, settings: &AppSettings) -> std::io::Result<()> {
    let args = build_launch_args(settings);
    let mut cmd = Command::new(&exe_path);
    cmd.args(args);
    if let Some(dir) = exe_path.parent() { cmd.current_dir(dir); }
    let _ = cmd.spawn()?;
    Ok(())
}

#[cfg(unix)]
fn detect_linux_steam_root(settings: &AppSettings) -> Option<PathBuf> {
    if let Some(override_path) = &settings.linux_steam_root_override {
        let p = PathBuf::from(override_path);
        if p.exists() { return Some(p); }
    }
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        let home = PathBuf::from(home);
        roots.push(home.join(".local/share/Steam"));
        roots.push(home.join(".steam/steam"));
        roots.push(home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"));
    }
    roots.push(PathBuf::from("/usr/lib/steam"));
    roots.into_iter().find(|r| r.exists())
}

#[cfg(unix)]
fn detect_linux_proton(settings: &AppSettings, steam_root: &PathBuf) -> Option<PathBuf> {
    if let Some(user) = &settings.linux_proton_path { let p = PathBuf::from(user); if p.exists() { return Some(p); } }
    let mut candidates: Vec<PathBuf> = Vec::new();
    // Official Proton installs
    candidates.push(steam_root.join("steamapps/common/Proton - Experimental/proton"));
    candidates.push(steam_root.join("steamapps/common/Proton - Hotfix/proton"));
    // In case Steam uses a numbered Proton (e.g., Proton 9)
    if let Ok(read) = std::fs::read_dir(steam_root.join("steamapps/common")) {
        for entry in read.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("Proton ") {
                let p = entry.path().join("proton");
                if p.exists() { candidates.push(p); }
            }
        }
    }
    // Proton-GE in user compatibilitytools.d
    let compat_dirs = [
        steam_root.join("compatibilitytools.d"),
        PathBuf::from("~/.local/share/Steam/compatibilitytools.d"),
        PathBuf::from("~/.steam/root/compatibilitytools.d"),
        PathBuf::from("~/.steam/steam/compatibilitytools.d"),
        PathBuf::from("~/.var/app/com.valvesoftware.Steam/.local/share/Steam/compatibilitytools.d"),
    ];
    for dir in compat_dirs.iter() {
        let d = shellexpand::tilde(&dir.display().to_string()).to_string();
        let d = PathBuf::from(d);
        if d.is_dir() {
            if let Ok(read) = std::fs::read_dir(&d) {
                for entry in read.flatten() {
                    let p = entry.path().join("proton");
                    if p.exists() { candidates.push(p); }
                }
            }
        }
    }
    // Last resort: try PATH for a `proton` executable
    if let Ok(p) = which::which("proton") { candidates.push(p); }
    candidates.into_iter().find(|p| p.exists())
}

#[cfg(unix)]
pub fn list_proton_builds(settings: &AppSettings) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    if let Some(root) = detect_linux_steam_root(settings) {
        // Official common dir
        let common = root.join("steamapps/common");
        if let Ok(read) = std::fs::read_dir(&common) {
            for entry in read.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("Proton ") || name.starts_with("Proton - ") {
                    let p = entry.path().join("proton");
                    if p.exists() {
                        out.push((name.clone(), p.display().to_string()));
                    }
                }
            }
        }
        // Proton-GE
        let compat_dirs = [
            root.join("compatibilitytools.d"),
            PathBuf::from("~/.local/share/Steam/compatibilitytools.d"),
            PathBuf::from("~/.steam/root/compatibilitytools.d"),
            PathBuf::from("~/.steam/steam/compatibilitytools.d"),
            PathBuf::from("~/.var/app/com.valvesoftware.Steam/.local/share/Steam/compatibilitytools.d"),
        ];
        for dir in compat_dirs.iter() {
            let d = shellexpand::tilde(&dir.display().to_string()).to_string();
            let d = PathBuf::from(d);
            if d.is_dir() {
                if let Ok(read) = std::fs::read_dir(&d) {
                    for entry in read.flatten() {
                        let label = entry.file_name().to_string_lossy().to_string();
                        let p = entry.path().join("proton");
                        if p.exists() { out.push((label, p.display().to_string())); }
                    }
                }
            }
        }
    }
    if let Ok(p) = which::which("proton") {
        out.push(("PATH: proton".to_string(), p.display().to_string()));
    }
    // Dedup by path, keep first occurrence (prefer official order)
    let mut seen = std::collections::HashSet::new();
    out.retain(|(_, path)| seen.insert(path.clone()));
    out
}

#[cfg(unix)]
pub fn launch_game(exe_path: PathBuf, settings: &AppSettings) -> std::io::Result<()> {
    let args = build_launch_args(settings);
    let Some(parent_dir) = exe_path.parent().map(|p| p.to_path_buf()) else { return Err(std::io::Error::new(std::io::ErrorKind::Other, "invalid exe path")); };
    let steam_root = detect_linux_steam_root(settings)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Steam root not found"))?;
    let compat = steam_root.join("steamapps/compatdata/4000");
    // Ensure compatdata dir exists so Proton/Steam can set up the prefix
    let _ = std::fs::create_dir_all(&compat);

    // Direct Proton invocation
    let proton = detect_linux_proton(settings, &steam_root)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Proton not found"))?;
    // Best-effort ensure Steam client is running so SteamAPI can initialize
    if let Ok(steam_bin) = which::which("steam") {
        let _ = std::process::Command::new(steam_bin).arg("-silent").spawn();
        // a brief delay can help SteamAPI attach; non-blocking preferred, so skip sleep here
    }
    let mut cmd = Command::new(&proton);
    cmd.arg("run");
    // Steam likes exe path relative to the game root; Proton `run` accepts abs. Keep abs path.
    cmd.arg(&exe_path);
    cmd.args(args);
    cmd.current_dir(&parent_dir);
    cmd.env("STEAM_COMPAT_CLIENT_INSTALL_PATH", &steam_root);
    cmd.env("STEAM_COMPAT_DATA_PATH", &compat);
    cmd.env("WINEDLLOVERRIDES", "d3d9=n,b");
    // Provide Steam App ID hints and steam_appid.txt to satisfy SteamAPI
    cmd.env("SteamAppId", "4000");
    cmd.env("SteamAppID", "4000");
    cmd.env("SteamGameId", "4000");
    cmd.env("SteamOverlayGameId", "4000");
    let _ = std::fs::write(parent_dir.join("steam_appid.txt"), b"4000\n");
    if settings.linux_enable_proton_log { cmd.env("PROTON_LOG", "1"); }
    let _ = cmd.spawn()?;
    Ok(())
}


