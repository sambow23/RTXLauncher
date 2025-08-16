use std::path::PathBuf;
use std::fs;
use std::env;

// Minimal Windows-only heuristic: default Program Files (x86) Steam, parse libraryfolders.vdf quickly.
#[cfg(windows)]
pub fn detect_gmod_install_folder() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    // Default Steam path
    if let Some(pf86) = option_env!("ProgramFiles(x86)").map(PathBuf::from) {
        let def = pf86.join("Steam");
        if def.exists() { candidates.push(def); }
    }
    // Fallback: C:\Program Files (x86)\Steam
    candidates.push(PathBuf::from("C:/Program Files (x86)/Steam"));

    for steam_root in candidates {
        let common = steam_root.join("steamapps").join("common");
        let gmod = common.join("GarrysMod");
        if gmod.exists() { return Some(gmod); }
        // Parse libraryfolders.vdf for additional libraries
        let vdf = steam_root.join("steamapps").join("libraryfolders.vdf");
        if let Ok(text) = fs::read_to_string(&vdf) {
            for line in text.lines() {
                if line.contains("\"path\"") {
                    if let Some(start) = line.find('"') {
                        if let Some(end) = line[start+1..].find('"') {
                            let p = &line[start+1..start+1+end];
                            let p = p.replace('/', "\\");
                            let path = PathBuf::from(p);
                            let gmod = path.join("steamapps").join("common").join("GarrysMod");
                            if gmod.exists() { return Some(gmod); }
                        }
                    }
                }
            }
        }
    }
    None
}

#[cfg(windows)]
pub fn detect_install_folder_path(install_folder: &str) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(pf86) = std::env::var("ProgramFiles(x86)") {
        let def = PathBuf::from(pf86).join("Steam");
        candidates.push(def);
    }
    candidates.push(PathBuf::from("C:/Program Files (x86)/Steam"));

    for steam_root in candidates {
        let common = steam_root.join("steamapps").join("common");
        let path = common.join(install_folder);
        if path.exists() { return Some(path); }
        let vdf = steam_root.join("steamapps").join("libraryfolders.vdf");
        if let Ok(text) = fs::read_to_string(&vdf) {
            for line in text.lines() {
                if line.contains("\"path\"") {
                    if let Some(start) = line.find('"') {
                        if let Some(end) = line[start+1..].find('"') {
                            let p = &line[start+1..start+1+end];
                            let p = p.replace('/', "\\");
                            let path = PathBuf::from(p).join("steamapps").join("common").join(install_folder);
                            if path.exists() { return Some(path); }
                        }
                    }
                }
            }
        }
    }
    None
}


#[cfg(unix)]
fn locate_in_steam_libraries(name: &str) -> Option<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        let home = PathBuf::from(home);
        roots.push(home.join(".local/share/Steam"));
        roots.push(home.join(".steam/steam"));
        roots.push(home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"));
    }
    // Common system path on some distros
    roots.push(PathBuf::from("/usr/lib/steam"));

    for root in roots {
        let candidate = root.join("steamapps").join("common").join(name);
        if candidate.exists() { return Some(candidate); }
        let vdf = root.join("steamapps").join("libraryfolders.vdf");
        if let Ok(text) = fs::read_to_string(&vdf) {
            for line in text.lines() {
                if line.contains("\"path\"") {
                    if let Some(start) = line.find('"') {
                        if let Some(end_rel) = line[start+1..].find('"') {
                            let path_str = &line[start+1..start+1+end_rel];
                            let lib_path = PathBuf::from(path_str).join("steamapps").join("common").join(name);
                            if lib_path.exists() { return Some(lib_path); }
                        }
                    }
                }
            }
        }
    }
    None
}

#[cfg(unix)]
pub fn detect_gmod_install_folder() -> Option<PathBuf> {
    locate_in_steam_libraries("GarrysMod")
}

#[cfg(unix)]
pub fn detect_install_folder_path(install_folder: &str) -> Option<PathBuf> {
    locate_in_steam_libraries(install_folder)
}

