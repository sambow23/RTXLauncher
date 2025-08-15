use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct FileUpdateInfo {
    pub relative_path: String,
    pub source_path: PathBuf,
    pub destination_path: PathBuf,
    pub is_directory: bool,
    pub is_new: bool,
    pub is_changed: bool,
}

fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

pub fn detect_updates(source_dir: &Path, dest_dir: &Path) -> Result<Vec<FileUpdateInfo>> {
    let mut result = Vec::new();
    let excluded_dirs = [
        "addons", "saves", "dupes", "demos", "settings", "cache",
        "materials", "models", "maps", "screenshots", "videos", "download",
    ];
    let excluded_ext = [".dem", ".log", ".vpk"];

    fn walk(
        source_root: &Path,
        dest_root: &Path,
        rel: &Path,
        result: &mut Vec<FileUpdateInfo>,
        excluded_dirs: &[&str],
        excluded_ext: &[&str],
    ) -> Result<()> {
        let here = source_root.join(rel);
        if !here.exists() { return Ok(()); }
        for entry in fs::read_dir(&here)? {
            let entry = entry?;
            let p = entry.path();
            let name = entry.file_name();
            let name_str = name.to_string_lossy().to_string();
            let rel_child = rel.join(&name);
            let dest_path = dest_root.join(&rel_child);
            if p.is_dir() {
                if rel.as_os_str().is_empty() && ["crashes","logs","temp","update","xenmod"].contains(&name_str.as_str()) {
                    continue;
                }
                if excluded_dirs.iter().any(|d| d.eq_ignore_ascii_case(&name_str)) { continue; }
                if !dest_path.exists() {
                    result.push(FileUpdateInfo { relative_path: rel_child.to_string_lossy().to_string(), source_path: p.clone(), destination_path: dest_path.clone(), is_directory: true, is_new: true, is_changed: false });
                }
                walk(source_root, dest_root, &rel_child, result, excluded_dirs, excluded_ext)?;
            } else {
                // root-level: only allow gmod.exe/hl2.exe
                if rel.as_os_str().is_empty() {
                    if name_str.to_lowercase() != "gmod.exe" && name_str.to_lowercase() != "hl2.exe" && name_str.to_lowercase() != "steam_appid.txt" { continue; }
                }
                if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                    if excluded_ext.iter().any(|x| x.trim_start_matches('.').eq_ignore_ascii_case(ext)) { continue; }
                }
                let is_new = !dest_path.exists();
                let is_changed = if is_new { false } else {
                    if is_symlink(&dest_path) { false } else {
                        let src_meta = fs::metadata(&p)?;
                        let dst_meta = fs::metadata(&dest_path)?;
                        let size_diff = src_meta.len() != dst_meta.len();
                        let time_diff = match (src_meta.modified().ok(), dst_meta.modified().ok()) {
                            (Some(a), Some(b)) => a != b,
                            _ => false,
                        };
                        size_diff || time_diff
                    }
                };
                if is_new || is_changed {
                    result.push(FileUpdateInfo {
                        relative_path: rel_child.to_string_lossy().to_string(),
                        source_path: p.clone(),
                        destination_path: dest_path.clone(),
                        is_directory: false,
                        is_new,
                        is_changed,
                    });
                }
            }
        }
        Ok(())
    }

    walk(source_dir, dest_dir, Path::new(""), &mut result, &excluded_dirs, &excluded_ext)?;
    Ok(result)
}

pub fn apply_updates(updates: &[FileUpdateInfo], mut progress: impl FnMut(&str, u8)) -> Result<()> {
    let total = updates.len().max(1);
    for (i, u) in updates.iter().enumerate() {
        let pct = ((i as f32 / total as f32) * 100.0) as u8;
        if u.is_directory {
            progress(&format!("Creating directory: {}", u.relative_path), pct);
            fs::create_dir_all(&u.destination_path)?;
        } else {
            progress(&format!("Copying file: {}", u.relative_path), pct);
            if let Some(parent) = u.destination_path.parent() { fs::create_dir_all(parent)?; }
            fs::copy(&u.source_path, &u.destination_path)?;
        }
    }
    progress("Update complete", 100);
    Ok(())
}


