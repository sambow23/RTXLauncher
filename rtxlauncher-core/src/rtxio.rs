use anyhow::{Result, Context};
use std::path::{Path, PathBuf};
use std::fs;
use std::process::Command;
use tracing::info;

pub fn has_rtxio_packages(game_install_path: &Path, remix_mod_folder: &str) -> bool {
    let remix_mod_path = game_install_path.join("rtx-remix").join("mods").join(remix_mod_folder);
    if !remix_mod_path.exists() { return false; }
    fs::read_dir(&remix_mod_path).map(|it| it.filter_map(|e| e.ok()).any(|e| e.path().extension().map(|x| x.eq("pkg")).unwrap_or(false))).unwrap_or(false)
}

fn default_extractor_path() -> PathBuf {
    let base = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())).unwrap_or_else(|| PathBuf::from("."));
    base.join("launcherdeps").join("rtxio").join("bin").join("RtxIoResourceExtractor.exe")
}

pub fn extract_packages(game_install_path: &Path, remix_mod_folder: &str, mut progress_cb: impl FnMut(&str, u8)) -> Result<bool> {
    let remix_mod_path = game_install_path.join("rtx-remix").join("mods").join(remix_mod_folder);
    if !remix_mod_path.exists() { return Ok(true); }

    let extractor = default_extractor_path();
    if !extractor.exists() {
        info!("RTXIO extractor not found: {}", extractor.display());
        progress_cb("RTXIO extractor not found. Place it at ./launcherdeps/rtxio/bin/RtxIoResourceExtractor.exe", 0);
        return Ok(false);
    }

    let pkg_files: Vec<PathBuf> = fs::read_dir(&remix_mod_path)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x.eq("pkg")).unwrap_or(false))
        .collect();
    if pkg_files.is_empty() { progress_cb("No .pkg files found", 100); return Ok(true); }

    let temp_out = std::env::temp_dir().join("rtxio_out");
    if temp_out.exists() { let _ = fs::remove_dir_all(&temp_out); }
    fs::create_dir_all(&temp_out).ok();

    for (i, pkg) in pkg_files.iter().enumerate() {
        let msg = format!("Extracting {} ({}/{})", pkg.file_name().unwrap().to_string_lossy(), i+1, pkg_files.len());
        info!("{}", msg);
        progress_cb(&msg, (i as u8 * 100 / pkg_files.len() as u8).min(95));
        let status = Command::new(&extractor)
            .arg(pkg)
            .arg("--force")
            .arg("-o")
            .arg(&temp_out)
            .status()
            .with_context(|| format!("run extractor for {}", pkg.display()))?;
        if !status.success() {
            progress_cb("RTXIO extractor failed", 0);
            return Ok(false);
        }
    }

    // Copy extracted files into remix mod path
    let _ = crate::fs_linker::copy_dir_recursive(&temp_out, &remix_mod_path)?;
    // Remove pkgs
    for pkg in pkg_files { let _ = fs::remove_file(pkg); }
    let _ = fs::remove_dir_all(&temp_out);
    progress_cb("RTXIO package extraction completed", 100);
    Ok(true)
}


