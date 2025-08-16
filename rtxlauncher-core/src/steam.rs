use std::path::PathBuf;
use std::fs;

/// Parse Steam library folders from the contents of a libraryfolders.vdf file.
///
/// Supports both old and new VDF layouts:
/// - Old: "1" "D:\\SteamLibrary"
/// - New: nested blocks with a "path" entry.
#[cfg(windows)]
fn parse_libraryfolders_vdf_paths(text: &str) -> Vec<PathBuf> {
    fn unescape_vdf_value_windows(raw: &str) -> String {
        let mut out = String::with_capacity(raw.len());
        let mut it = raw.chars().peekable();
        while let Some(c) = it.next() {
            if c == '\\' {
                if let Some(n) = it.next() {
                    match n {
                        '\\' => out.push('\\'),
                        '"' => out.push('"'),
                        'n' => out.push('\n'),
                        'r' => out.push('\r'),
                        't' => out.push('\t'),
                        _ => out.push(n),
                    }
                } else {
                    out.push('\\');
                }
            } else {
                out.push(c);
            }
        }
        out
    }
    let mut results: Vec<PathBuf> = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        // New format: ... "path" "C:\\..." ...
        if l.contains("\"path\"") {
            if let Some(first_q) = l.find('"') {
                if let Some(rest) = l.get(first_q + 1..) {
                    if let Some(second_rel) = rest.find('"') {
                        if let Some(value_seg) = rest.get(second_rel + 1..) {
                            if let Some(value_start) = value_seg.find('"') {
                                if let Some(value_end_rel) = value_seg.get(value_start + 1..).and_then(|s| s.find('"')) {
                                    let raw = &value_seg[value_start + 1..value_start + 1 + value_end_rel];
                                    let unescaped = unescape_vdf_value_windows(raw);
                                    let normalized = unescaped.replace('/', "\\");
                                    let p = PathBuf::from(normalized);
                                    if !results.contains(&p) { results.push(p); }
                                    continue;
                                }
                            }
                        }
                    }
                }
            }
        }
        // Old format: "<digits>" "<path>"
        if l.starts_with('"') {
            let mut parts = l.split('"').filter(|s| !s.is_empty());
            if let (Some(key), Some(val)) = (parts.next(), parts.nth(1)) { // skip the quote between key and value
                if key.chars().all(|c| c.is_ascii_digit()) {
                    let unescaped = unescape_vdf_value_windows(val);
                    let normalized = unescaped.replace('/', "\\");
                    let p = PathBuf::from(normalized);
                    if !results.contains(&p) { results.push(p); }
                }
            }
        }
    }
    results
}

#[cfg(unix)]
fn parse_libraryfolders_vdf_paths(text: &str) -> Vec<PathBuf> {
    let mut results: Vec<PathBuf> = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        // New format: ... "path" "/home/..." ...
        if l.contains("\"path\"") {
            if let Some(first_q) = l.find('"') {
                if let Some(rest) = l.get(first_q + 1..) {
                    if let Some(second_rel) = rest.find('"') {
                        if let Some(value_seg) = rest.get(second_rel + 1..) {
                            if let Some(value_start) = value_seg.find('"') {
                                if let Some(value_end_rel) = value_seg.get(value_start + 1..).and_then(|s| s.find('"')) {
                                    let raw = &value_seg[value_start + 1..value_start + 1 + value_end_rel];
                                    let p = PathBuf::from(raw);
                                    if !results.contains(&p) { results.push(p); }
                                    continue;
                                }
                            }
                        }
                    }
                }
            }
        }
        // Old format: "<digits>" "<path>"
        if l.starts_with('"') {
            let mut parts = l.split('"').filter(|s| !s.is_empty());
            if let (Some(key), Some(val)) = (parts.next(), parts.nth(1)) {
                if key.chars().all(|c| c.is_ascii_digit()) {
                    let p = PathBuf::from(val);
                    if !results.contains(&p) { results.push(p); }
                }
            }
        }
    }
    results
}

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
            for lib_root in parse_libraryfolders_vdf_paths(&text) {
                let gmod = lib_root.join("steamapps").join("common").join("GarrysMod");
                if gmod.exists() { return Some(gmod); }
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
            for lib_root in parse_libraryfolders_vdf_paths(&text) {
                let path = lib_root.join("steamapps").join("common").join(install_folder);
                if path.exists() { return Some(path); }
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
            for lib_root in parse_libraryfolders_vdf_paths(&text) {
                let lib_path = lib_root.join("steamapps").join("common").join(name);
                if lib_path.exists() { return Some(lib_path); }
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

#[cfg(test)]
mod tests {
    use super::parse_libraryfolders_vdf_paths;
    use std::path::PathBuf;

    #[cfg(windows)]
    #[test]
    fn parse_vdf_paths_windows_mixed_formats() {
        let vdf = r#"
        "LibraryFolders"
        {
            "contentstatsid" "-123456789"
            "1" "D:\\SteamLibrary"
            "2"
            {
                "path" "E:\\Games\\SteamLibrary"
                "label" ""
                "contentid" "123456789"
            }
        }
        "#;
        let libs = parse_libraryfolders_vdf_paths(vdf);
        assert!(libs.contains(&PathBuf::from("D:\\SteamLibrary")));
        assert!(libs.contains(&PathBuf::from("E:\\Games\\SteamLibrary")));
    }

    #[cfg(unix)]
    #[test]
    fn parse_vdf_paths_unix_mixed_formats() {
        let vdf = r#"
        "LibraryFolders"
        {
            "contentstatsid" "-123456789"
            "1" "/mnt/ssd/SteamLibrary"
            "2"
            {
                "path" "/home/user/.steam/steamapps/compat/SteamLibrary"
                "label" ""
                "contentid" "123456789"
            }
        }
        "#;
        let libs = parse_libraryfolders_vdf_paths(vdf);
        assert!(libs.contains(&PathBuf::from("/mnt/ssd/SteamLibrary")));
        assert!(libs.contains(&PathBuf::from("/home/user/.steam/steamapps/compat/SteamLibrary")));
    }
}

