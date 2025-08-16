use anyhow::Result;
use std::path::{Path, PathBuf};
use std::fs;
use crate::fs_linker::{link_dir_best_effort, link_file_best_effort, copy_dir_with_progress};
use tracing::info;

fn flatten_if_nested(dir: &Path) -> Result<()> {
    // If <dir>/<basename(dir)> exists, move its children up one level and remove the nested folder
    if !dir.exists() { return Ok(()); }
    if let Some(name) = dir.file_name() {
        let nested = dir.join(name);
        if nested.exists() && nested.is_dir() {
            for entry in std::fs::read_dir(&nested)? {
                let entry = entry?;
                let from = entry.path();
                let to = dir.join(entry.file_name());
                std::fs::create_dir_all(to.parent().unwrap_or(dir)).ok();
                if std::fs::rename(&from, &to).is_err() {
                    if from.is_dir() {
                        let _ = crate::fs_linker::copy_dir_recursive(&from, &to);
                        let _ = std::fs::remove_dir_all(&from);
                    } else {
                        let _ = std::fs::copy(&from, &to);
                        let _ = std::fs::remove_file(&from);
                    }
                }
            }
            let _ = std::fs::remove_dir_all(&nested);
        }
    }
    Ok(())
}

pub struct InstallPlan {
    pub vanilla: PathBuf,
    pub rtx: PathBuf,
}

pub fn perform_basic_install(plan: &InstallPlan, mut progress_cb: impl FnMut(&str, u8)) -> Result<()> {
    let mut progress = |m: &str, pct: u8| { info!("{}", m); progress_cb(m, pct); };
    progress("Starting install", 0);

    // 1. Copy bin folder (ensure layout: <rtx>/bin/<files> and <rtx>/bin/win64/<files>)
    progress("Copying bin folder", 10);
    let src_bin = plan.vanilla.join("bin");
    let dst_bin = plan.rtx.join("bin");
    copy_dir_with_progress(&src_bin, &dst_bin, |_c, _t| {})?;
    // Fix nested copies if any (bin/bin)
    let _ = flatten_if_nested(&dst_bin);
    // If a win64 exists in the vanilla bin, ensure it is present in destination
    let src_win64 = src_bin.join("win64");
    if src_win64.exists() {
        let dst_win64 = dst_bin.join("win64");
        copy_dir_with_progress(&src_win64, &dst_win64, |_c, _t| {})?;
        let _ = flatten_if_nested(&dst_win64);
    }

    // 2. Ensure garrysmod folder
    let rtx_gm = plan.rtx.join("garrysmod");
    fs::create_dir_all(&rtx_gm)?;
    let _ = flatten_if_nested(&rtx_gm);

    // 3. Copy gmod.exe or fallback hl2.exe to root; if 64-bit layout present, prefer bin/win64 exe as well
    progress("Copying executable", 20);
    let root_exe_src = if plan.vanilla.join("gmod.exe").exists() {
        plan.vanilla.join("gmod.exe")
    } else {
        plan.vanilla.join("hl2.exe")
    };
    let root_exe_dst = plan.rtx.join(root_exe_src.file_name().unwrap());
    if root_exe_src.exists() { let _ = std::fs::copy(&root_exe_src, &root_exe_dst); }
    // Also copy win64 gmod.exe if present
    let win64_exe_src = plan.vanilla.join("bin").join("win64").join("gmod.exe");
    if win64_exe_src.exists() {
        let _ = std::fs::copy(&win64_exe_src, &plan.rtx.join("bin").join("win64").join("gmod.exe"));
    }

    // 4. Copy steam_appid.txt if present
    let appid_src = plan.vanilla.join("steam_appid.txt");
    if appid_src.exists() { std::fs::copy(&appid_src, &plan.rtx.join("steam_appid.txt"))?; }

    // 5. Symlink VPK files in garrysmod root
    progress("Linking VPK files", 30);
    for entry in fs::read_dir(plan.vanilla.join("garrysmod"))? {
        let entry = entry?;
        if let Some(ext) = entry.path().extension() {
            if ext.eq_ignore_ascii_case("vpk") {
                let dst = rtx_gm.join(entry.file_name());
                let _ = link_file_best_effort(&entry.path(), &dst);
            }
        }
    }

    // 6. Link external folders sourceengine, platform
    progress("Linking external folders", 40);
    for folder in ["sourceengine", "platform"] {
        let src = plan.vanilla.join(folder);
        let dst = plan.rtx.join(folder);
        if src.exists() { let _ = link_dir_best_effort(&src, &dst); }
        let _ = flatten_if_nested(&dst);
    }

    // 7/8 Excluded folders and copy rest of garrysmod top-level files (except excluded ext)
    progress("Copying garrysmod contents", 60);
    let excluded_dirs = [
        "addons","saves","dupes","demos","settings","cache",
        "materials","models","maps","screenshots","videos","download"
    ];
    let excluded_ext = ["dem","log"];

    // files in garrysmod root
    for entry in fs::read_dir(plan.vanilla.join("garrysmod"))? {
        let entry = entry?;
        let p = entry.path();
        if p.is_file() {
            if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                if excluded_ext.iter().any(|x| x.eq_ignore_ascii_case(ext)) { continue; }
            }
            let dst = rtx_gm.join(entry.file_name());
            if !dst.exists() { let _ = std::fs::copy(&p, &dst); }
        }
    }
    // directories in garrysmod
    for entry in fs::read_dir(plan.vanilla.join("garrysmod"))? {
        let entry = entry?;
        let p = entry.path();
        if p.is_dir() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if excluded_dirs.iter().any(|d| d.eq_ignore_ascii_case(&name_str)) { continue; }
            let dst = rtx_gm.join(&name);
            let _ = copy_dir_with_progress(&p, &dst, |_c, _t| {});
            let _ = flatten_if_nested(&dst);
        }
    }

    // 9. Create blank addons
    fs::create_dir_all(rtx_gm.join("addons"))?;

    // 10. Symlink selected garrysmod subfolders (match C# Quick Install behavior)
    // Includes content-heavy folders to avoid duplicating large data
    for folder in [
        "saves", "dupes", "demos", "settings", "cache", "download",
        "materials", "models", "maps", "screenshots", "videos"
    ] {
        let src = plan.vanilla.join("garrysmod").join(folder);
        let dst = rtx_gm.join(folder);
        if src.exists() { let _ = link_dir_best_effort(&src, &dst); }
    }

    progress("Install complete", 100);
    Ok(())
}


