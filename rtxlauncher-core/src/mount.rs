use anyhow::{Result, Context};
use std::fs;
use std::path::{Path, PathBuf};
use crate::fs_linker::{link_dir_best_effort};
use tracing::info;

fn get_this_install_folder() -> Result<PathBuf> {
    let exe = std::env::current_exe()?;
    Ok(exe.parent().unwrap().to_path_buf())
}

pub fn is_game_mounted(game_folder: &str, install_folder: &str, remix_mod_folder: &str) -> bool {
    if let Ok(gmod_path) = get_this_install_folder() {
        let src_mount = gmod_path.join("garrysmod").join("addons").join(format!("mount-{}", game_folder));
        let remix_mount = gmod_path.join("rtx-remix").join("mods").join(format!("mount-{}-{}", game_folder, remix_mod_folder));
        return src_mount.exists() && remix_mount.exists();
    }
    false
}

pub fn mount_game(game_folder: &str, install_folder: &str, remix_mod_folder: &str, mut progress_cb: impl FnMut(&str)) -> Result<()> {
    let mut progress = |m: &str| { info!("{}", m); progress_cb(m); };
    progress("Mounting content...");
    let gmod_path = get_this_install_folder()?;
    let install_path = find_install_folder(install_folder).with_context(|| format!("Install folder '{}' not found", install_folder))?;

    // Source content
    let source_content_path = install_path.join(game_folder);
    let source_content_mount_path = gmod_path.join("garrysmod").join("addons").join(format!("mount-{}", game_folder));
    fs::create_dir_all(&source_content_mount_path)?;

    // Link models
    let models = source_content_path.join("models");
    if models.exists() { let _ = link_dir_best_effort(&models, &source_content_mount_path.join("models")); }
    // Link maps
    let maps = source_content_path.join("maps");
    if maps.exists() { let _ = link_dir_best_effort(&maps, &source_content_mount_path.join("maps")); }
    // Link materials subfolders except excluded
    let materials = source_content_path.join("materials");
    if materials.exists() {
        let dst_mat = source_content_mount_path.join("materials");
        fs::create_dir_all(&dst_mat).ok();
        let dont_link = ["vgui", "dev", "editor", "perftest", "tools"];
        for entry in fs::read_dir(&materials)? {
            let entry = entry?;
            if entry.path().is_dir() {
                let name = entry.file_name();
                if dont_link.iter().any(|x| x.eq_ignore_ascii_case(&name.to_string_lossy())) { continue; }
                let _ = link_dir_best_effort(&entry.path(), &dst_mat.join(name));
            }
        }
    }

    // Custom content
    let custom = source_content_path.join("custom");
    if custom.exists() {
        for entry in fs::read_dir(&custom)? {
            let entry = entry?;
            if entry.path().is_dir() {
                let mount_dst = gmod_path.join("garrysmod").join("addons").join(format!("mount-{}-{}", game_folder, entry.file_name().to_string_lossy()));
                fs::create_dir_all(&mount_dst).ok();
                // link subfolders similar to base
                let models = entry.path().join("models");
                if models.exists() { let _ = link_dir_best_effort(&models, &mount_dst.join("models")); }
                let maps = entry.path().join("maps");
                if maps.exists() { let _ = link_dir_best_effort(&maps, &mount_dst.join("maps")); }
                let materials = entry.path().join("materials");
                if materials.exists() {
                    let dst_mat = mount_dst.join("materials");
                    fs::create_dir_all(&dst_mat).ok();
                    let dont_link = ["vgui", "dev", "editor", "perftest", "tools"];
                    for sub in fs::read_dir(&materials)? {
                        let sub = sub?;
                        if sub.path().is_dir() {
                            let name = sub.file_name();
                            if dont_link.iter().any(|x| x.eq_ignore_ascii_case(&name.to_string_lossy())) { continue; }
                            let _ = link_dir_best_effort(&sub.path(), &dst_mat.join(name));
                        }
                    }
                }
            }
        }
    }

    // Remix mod link
    let remix_mod_path = install_path.join("rtx-remix").join("mods").join(remix_mod_folder);
    let remix_mod_mount_path = gmod_path.join("rtx-remix").join("mods").join(format!("mount-{}-{}", game_folder, remix_mod_folder));
    fs::create_dir_all(remix_mod_mount_path.parent().unwrap()).ok();
    if remix_mod_path.exists() {
        let _ = link_dir_best_effort(&remix_mod_path, &remix_mod_mount_path);
    }

    progress("Mount complete");
    Ok(())
}

pub fn unmount_game(game_folder: &str, install_folder: &str, remix_mod_folder: &str, mut progress_cb: impl FnMut(&str)) -> Result<()> {
    let mut progress = |m: &str| { info!("{}", m); progress_cb(m); };
    progress("Unmounting...");
    let gmod_path = get_this_install_folder()?;
    let src_mount = gmod_path.join("garrysmod").join("addons").join(format!("mount-{}", game_folder));
    let remix_mount = gmod_path.join("rtx-remix").join("mods").join(format!("mount-{}-{}", game_folder, remix_mod_folder));
    if remix_mount.exists() { let _ = fs::remove_dir_all(&remix_mount); }
    if src_mount.exists() { let _ = fs::remove_dir_all(&src_mount); }
    // Remove custom mounts
    let addons = gmod_path.join("garrysmod").join("addons");
    if addons.exists() {
        for entry in fs::read_dir(&addons)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(&format!("mount-{}-", game_folder)) {
                let _ = fs::remove_dir_all(entry.path());
            }
        }
    }
    progress("Unmount complete");
    Ok(())
}

fn find_install_folder(install_folder: &str) -> Result<PathBuf> {
    // Try steam default locations quickly; reuse the minimal heuristic from steam.rs
    // For simplicity, check common library roots only.
    let mut roots = Vec::new();
    if let Ok(pf86) = std::env::var("ProgramFiles(x86)") { roots.push(PathBuf::from(pf86).join("Steam")); }
    roots.push(PathBuf::from("C:/Program Files (x86)/Steam"));
    for root in roots {
        let p = root.join("steamapps").join("common").join(install_folder);
        if p.exists() { return Ok(p); }
    }
    Err(anyhow::anyhow!("install folder not found"))
}


