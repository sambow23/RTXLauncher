use anyhow::Result;
use crate::github::{GitHubRelease, GitHubAsset};
use std::path::PathBuf;
use zip::ZipArchive;
use reqwest::Client;
use futures_util::StreamExt;
use std::io::Cursor;
use std::io::Read;
use std::fs::File;
use std::io::Write;
use std::fs::create_dir_all;
use tracing::info;
use crate::logging::ProgressThrottle;

pub fn select_best_asset(release: &GitHubRelease, prefer_gmod_zip: bool) -> Option<&GitHubAsset> {
    if prefer_gmod_zip {
        if let Some(a) = release.assets.iter().find(|a| a.name.ends_with("-gmod.zip")) { return Some(a); }
    }
    let patterns = ["-release.zip", "-debugoptimized.zip", "-debug.zip", ".zip"];
    for pat in patterns {
        if let Some(a) = release.assets.iter().find(|a| a.name.contains(pat) && !a.name.contains("-symbols")) { return Some(a); }
    }
    None
}

pub fn analyze_zip_for_layout<R: std::io::Read + std::io::Seek>(zip: &mut ZipArchive<R>) -> (bool, bool) {
    let mut has_trex = false;
    let mut has_d3d9 = false;
    for i in 0..zip.len() {
        if let Ok(f) = zip.by_index(i) {
            let name = f.name().to_string();
            if name.contains(".trex/") || name.contains(".trex\\") { has_trex = true; }
            if name.rsplit('/').next().unwrap_or("") == "d3d9.dll" || name.rsplit('\\').next().unwrap_or("") == "d3d9.dll" { has_d3d9 = true; }
        }
    }
    (has_trex, has_d3d9)
}

pub async fn install_remix_from_release(
    release: &GitHubRelease,
    rtx_root: &PathBuf,
    mut progress: impl FnMut(&str, u8),
) -> Result<()> {
    let mut progress_cb = |m: &str, pct: u8| { info!("{}", m); progress(m, pct); };
    progress_cb("Analyzing release assets", 5);
    // Prefer gmod zip for 64-bit if available
    let is64 = rtx_root.join("bin").join("win64").exists();
    let asset = select_best_asset(release, is64)
        .ok_or_else(|| anyhow::anyhow!("no suitable asset"))?;
    let url = asset.browser_download_url.clone().ok_or_else(|| anyhow::anyhow!("asset has no download url"))?;

    progress_cb(&format!("Downloading {}", asset.name), 10);
    let mut throttler = ProgressThrottle::new(150);
    let client = Client::new();
    let resp = client.get(&url).header("User-Agent", "RTXLauncher-RS").send().await?;
    let total = resp.content_length().unwrap_or(0);
    let mut bytes = resp.bytes_stream();
    let mut data: Vec<u8> = Vec::with_capacity(total as usize);
    let mut downloaded: u64 = 0;
    while let Some(chunk_res) = bytes.next().await {
        let chunk = chunk_res?;
        data.extend_from_slice(&chunk);
        downloaded += chunk.len() as u64;
        if total > 0 {
            let pct = 10 + ((downloaded as f32 / total as f32) * 50.0) as u8;
            let msg = format!("Downloading: {}/{} MB", downloaded/1_048_576, total/1_048_576);
            throttler.emit("Downloading:", msg, pct.min(60), |m,p| progress_cb(m,p));
        }
    }

    progress_cb("Analyzing package", 65);
    let mut cursor = Cursor::new(&data);
    let mut zip = ZipArchive::new(&mut cursor)?;
    let (_has_trex, _has_d3d9) = analyze_zip_for_layout(&mut zip);
    // reset cursor to re-open archive for extraction
    cursor.set_position(0);
    let mut zip = ZipArchive::new(cursor)?;

    let dest_path = if is64 { rtx_root.join("bin").join("win64") } else { rtx_root.join("bin") };
    create_dir_all(&dest_path).ok();

    progress_cb("Extracting files", 70);
    let total_files = zip.len();
    for i in 0..total_files {
        let mut file = zip.by_index(i)?;
        let raw_name = file.name().to_string();
        let name_norm = raw_name.replace('\\', "/");
        // For 64-bit installs, only extract content inside .trex/, stripping the prefix
        if is64 {
            if !name_norm.starts_with(".trex/") && !file.is_dir() { continue; }
        }
        // Determine relative path
        let rel = if is64 && name_norm.starts_with(".trex/") { &name_norm[6..] } else { &name_norm };
        if rel.is_empty() { continue; }
        let outpath = dest_path.join(rel.replace(':', "_"));

        if file.is_dir() {
            create_dir_all(&outpath).ok();
        } else {
            if let Some(parent) = outpath.parent() { create_dir_all(parent).ok(); }
            let mut outfile = File::create(&outpath)?;
            std::io::copy(&mut file, &mut outfile)?;
        }
        let pct = 70 + (((i as f32 + 1.0) / (total_files as f32)) * 25.0) as u8;
        progress_cb("Extracting...", pct.min(95));
    }

    progress_cb("RTX Remix installed", 100);
    Ok(())
}


// Select a package asset prioritizing "-launcher.zip" then any ".zip"
pub fn select_best_package_asset(release: &GitHubRelease) -> Option<&GitHubAsset> {
    if let Some(a) = release.assets.iter().find(|a| a.name.ends_with("-launcher.zip")) { return Some(a); }
    release.assets.iter().find(|a| a.name.ends_with(".zip"))
}

fn normalize_path_for_match(p: &str) -> String {
    let mut s = p.replace('\\', "/");
    if s.starts_with('/') { s = s.trim_start_matches('/').to_string(); }
    s
}

fn parse_ignore_patterns(text: &str) -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') { continue; }
        set.insert(normalize_path_for_match(t));
    }
    set
}

fn should_ignore(path: &str, ignored: &std::collections::HashSet<String>) -> bool {
    let norm = normalize_path_for_match(path);
    if ignored.contains(&norm) { return true; }
    for pat in ignored.iter() {
        if let Some(prefix) = pat.strip_suffix("/*") {
            if norm.starts_with(prefix) { return true; }
        }
    }
    false
}

/// Install a generic fixes package from a GitHub release into the install directory
/// Respects default ignore patterns and optional .launcherignore contained inside the zip
pub async fn install_fixes_from_release(
    release: &GitHubRelease,
    install_dir: &PathBuf,
    default_ignore_patterns: Option<&str>,
    mut progress: impl FnMut(&str, u8),
) -> Result<()> {
    let mut progress_cb = |m: &str, pct: u8| { info!("{}", m); progress(m, pct); };
    progress_cb("Analyzing release assets", 5);
    let asset = select_best_package_asset(release)
        .ok_or_else(|| anyhow::anyhow!("no suitable package asset"))?;
    let url = asset.browser_download_url.clone().ok_or_else(|| anyhow::anyhow!("asset has no download url"))?;

    progress_cb(&format!("Downloading {}", asset.name), 10);
    let mut throttler = ProgressThrottle::new(150);
    let client = Client::new();
    let resp = client.get(&url).header("User-Agent", "RTXLauncher-RS").send().await?;
    let total = resp.content_length().unwrap_or(0);
    let mut bytes = resp.bytes_stream();
    let mut data: Vec<u8> = Vec::with_capacity(total as usize);
    let mut downloaded: u64 = 0;
    while let Some(chunk_res) = bytes.next().await {
        let chunk = chunk_res?;
        data.extend_from_slice(&chunk);
        downloaded += chunk.len() as u64;
        if total > 0 {
            let pct = 10 + ((downloaded as f32 / total as f32) * 40.0) as u8;
            let msg = format!("Downloading: {}/{} MB", downloaded/1_048_576, total/1_048_576);
            throttler.emit("Downloading:", msg, pct.min(50), |m,p| progress_cb(m,p));
        }
    }

    progress_cb("Checking package contents", 52);
    let mut cursor = Cursor::new(&data);
    let mut zip = ZipArchive::new(&mut cursor)?;

    // Build ignore set: default + .launcherignore if present
    let mut ignored = std::collections::HashSet::new();
    if let Some(def) = default_ignore_patterns { ignored.extend(parse_ignore_patterns(def)); }

    // Attempt to read .launcherignore without extracting to disk
    for i in 0..zip.len() {
        let mut f = zip.by_index(i)?;
        let name = f.name().to_string();
        if name == ".launcherignore" || name.ends_with("/.launcherignore") {
            let mut s = String::new();
            let _ = f.read_to_string(&mut s);
            for p in parse_ignore_patterns(&s) { ignored.insert(p); }
            break;
        }
    }

    // Reset to extract pass
    cursor.set_position(0);
    let mut zip = ZipArchive::new(cursor)?;

    progress_cb("Extracting files", 60);
    let total_files = zip.len();
    for i in 0..total_files {
        let mut file = zip.by_index(i)?;
        let name = file.name().to_string();
        if should_ignore(&name, &ignored) { continue; }

        let outpath = install_dir.join(name.replace(':', "_").replace("\\", "/"));
        if file.is_dir() {
            create_dir_all(&outpath).ok();
        } else {
            if let Some(parent) = outpath.parent() { create_dir_all(parent).ok(); }
            let mut outfile = File::create(&outpath)?;
            std::io::copy(&mut file, &mut outfile)?;
        }
        let pct = 60 + (((i as f32 + 1.0) / (total_files as f32)) * 35.0) as u8;
        progress_cb("Extracting...", pct.min(95));
    }

    progress_cb("Fixes package installed", 100);
    Ok(())
}


