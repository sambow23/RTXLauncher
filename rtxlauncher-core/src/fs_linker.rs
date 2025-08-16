use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

#[cfg(windows)]
use std::os::windows::fs as winfs;

/// Attempt to create a directory link from dst -> src.
/// Strategy: symlink_dir -> junction -> copy (fallback).
pub fn link_dir_best_effort(src: &Path, dst: &Path) -> Result<()> {
    // Ensure parent exists
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create parent for {}", dst.display()))?;
    }

    // If already exists, do nothing
    if dst.exists() {
        return Ok(());
    }

    // Try symlink
    #[cfg(windows)]
    {
        if let Err(_e) = winfs::symlink_dir(src, dst) {
            // Try junction as fallback
            if let Err(e2) = junction::create(dst, src) {
                // Last resort: copy
                let _ = copy_dir_recursive(src, dst)
                    .with_context(|| format!("junction failed: {e2}; copied instead"))?;
            }
        }
        return Ok(());
    }

    #[cfg(not(windows))]
    {
        // Non-Windows: symlink_dir
        std::os::unix::fs::symlink(src, dst)
            .or_else(|_| copy_dir_recursive(src, dst).map(|_| ()))?;
        return Ok(());
    }
}

/// Attempt to create a file link from dst -> src.
/// Strategy: symlink_file -> copy fallback.
pub fn link_file_best_effort(src: &Path, dst: &Path) -> Result<()> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create parent for {}", dst.display()))?;
    }
    if dst.exists() {
        return Ok(());
    }

    #[cfg(windows)]
    {
        if let Err(_e) = winfs::symlink_file(src, dst) {
            fs::copy(src, dst).with_context(|| format!("copy {} -> {}", src.display(), dst.display()))?;
        }
        return Ok(());
    }
    #[cfg(not(windows))]
    {
        if let Err(_e) = std::os::unix::fs::symlink(src, dst) {
            fs::copy(src, dst).with_context(|| format!("copy {} -> {}", src.display(), dst.display()))?;
        }
        return Ok(());
    }
}

/// Basic recursive copy (no progress). Use fs_extra for robustness.
pub fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<u64> {
    use fs_extra::dir::{copy, CopyOptions};
    let mut opts = CopyOptions::new();
    opts.copy_inside = true;
    opts.overwrite = true;
    fs::create_dir_all(dst).ok();
    let n = copy(src, dst, &opts).with_context(|| format!("copy dir {} -> {}", src.display(), dst.display()))?;
    Ok(n)
}

/// Recursive copy with simple progress callback (0..=100 is up to caller).
/// We report best-effort progress based on bytes.
pub fn copy_dir_with_progress<F: FnMut(u64, u64)>(src: &Path, dst: &Path, mut on_progress: F) -> Result<u64> {
    use fs_extra::dir::{copy_with_progress, CopyOptions, TransitProcess};
    let mut opts = CopyOptions::new();
    opts.copy_inside = true;
    opts.overwrite = true;
    fs::create_dir_all(dst).ok();
    let handler = |tp: TransitProcess| {
        on_progress(tp.copied_bytes, tp.total_bytes);
        fs_extra::dir::TransitProcessResult::ContinueOrAbort
    };
    let n = copy_with_progress(src, dst, &opts, handler)
        .with_context(|| format!("copy (progress) {} -> {}", src.display(), dst.display()))?;
    Ok(n)
}


