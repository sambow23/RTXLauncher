use anyhow::{Result, Context};
use reqwest::Client;
use std::{collections::{HashMap}, path::Path};

#[derive(Debug, Clone, Default)]
pub struct PatchResult {
    pub files_patched: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
struct PatternSpec {
    hex_mask: String,
    offset: isize,
    override_hex: Option<String>,
}

#[derive(Debug, Clone)]
struct PatchSet {
    patterns: Vec<PatternSpec>,
    default_replacement: Option<String>,
}

type PatchMap = HashMap<String, Vec<PatchSet>>;

fn strip_comments(src: &str) -> String {
    // Remove Python comments starting with '#', keep line breaks
    src.lines().map(|l| {
        if let Some(i) = l.find('#') { &l[..i] } else { l }
    }).collect::<Vec<_>>().join("\n")
}

fn parse_patches_from_python(src: &str) -> Result<(PatchMap, PatchMap)> {
    // Very small, tailored parser that extracts two dict literals: patches32 = {...} and patches64 = {...}
    // We convert them into our PatchMap structures. We assume the script structure used by SourceRTXTweaks.
    let text = strip_comments(src);
    let find_dict = |name: &str| -> Result<&str> {
        let start_tag = format!("{} = {{", name);
        let start_pos = if let Some(pos) = text.find(&start_tag) { pos + start_tag.len() - 1 } else {
            // allow variant without spaces: name={
            let alt = format!("{}={{", name);
            text.find(&alt).ok_or_else(|| anyhow::anyhow!("{} not found", name))? + alt.len()-1
        };
        // naive brace matching
        let bytes = text.as_bytes();
        let mut depth = 0i32;
        let mut end_idx = None;
        for (i, &b) in bytes[start_pos..].iter().enumerate() {
            let c = b as char;
            if c == '{' { depth += 1; }
            if c == '}' { depth -= 1; if depth == 0 { end_idx = Some(start_pos + i + 1); break; } }
        }
        let end = end_idx.ok_or_else(|| anyhow::anyhow!("{} unmatched braces", name))?;
        Ok(&text[start_pos..end])
    };

    fn parse_dict(body: &str) -> Result<PatchMap> {
        // We will scan keys '...': [ ... ] entries.
        let mut map: PatchMap = HashMap::new();
        // Strip outer braces if present
        let trimmed = body.trim();
        let slice: &str = if trimmed.starts_with('{') && trimmed.ends_with('}') {
            &trimmed[1..trimmed.len()-1]
        } else { trimmed };
        // Split top-level entries by '],', account for nested brackets by depth counters.
        let mut i = 0usize;
        let chars: Vec<char> = slice.chars().collect();
        while i < chars.len() {
            // skip whitespace and commas
            while i < chars.len() && chars[i].is_whitespace() { i += 1; }
            if i >= chars.len() { break; }
            if chars[i] == '}' { break; }
            // expect key: '...'
            if chars[i] != '\'' { return Err(anyhow::anyhow!("expected quoted key")); }
            i += 1; let start_key = i; while i < chars.len() && chars[i] != '\'' { i += 1; }
            let key = chars[start_key..i].iter().collect::<String>();
            i += 1; // closing quote
            // skip spaces and ':'
            while i < chars.len() && (chars[i].is_whitespace() || chars[i] == ':' ) { if chars[i] == ':' { i += 1; break; } i += 1; }
            while i < chars.len() && chars[i].is_whitespace() { i += 1; }
            if i >= chars.len() || chars[i] != '[' { return Err(anyhow::anyhow!("expected [")); }
            // capture value list with bracket matching
            let mut depth = 0i32; let start_val = i; while i < chars.len() { let c = chars[i]; if c == '[' { depth += 1; } if c == ']' { depth -= 1; if depth == 0 { i += 1; break; } } i += 1; }
            let val = chars[start_val..i].iter().collect::<String>();
            // parse list of PatchSet
            let sets = parse_patch_sets(&val)?;
            map.insert(key, sets);
            // move past comma
            while i < chars.len() && chars[i] != '\'' { if chars[i] == ',' { i += 1; break; } i += 1; }
        }
        Ok(map)
    }

    fn parse_patch_sets(list_src: &str) -> Result<Vec<PatchSet>> {
        // list_src is like [ entry, entry, ... ] where each entry itself is a [ ... ] list
        let inner = &list_src[1..list_src.len()-1];
        let mut out = Vec::new();
        let mut i = 0usize; let chars: Vec<char> = inner.chars().collect();
        while i < chars.len() {
            while i < chars.len() && chars[i].is_whitespace() { i += 1; }
            if i >= chars.len() { break; }
            if chars[i] == '[' { // capture the whole entry list
                let mut depth = 0i32; let start = i; while i < chars.len() { let c = chars[i]; if c == '[' { depth += 1; } if c == ']' { depth -= 1; if depth == 0 { i += 1; break; } } i += 1; }
                let entry_src = chars[start..i].iter().collect::<String>();
                out.push(parse_entry_list(&entry_src)?);
            } else {
                // skip unexpected token conservatively to next comma
                while i < chars.len() && chars[i] != ',' { i += 1; }
            }
            // advance past comma if present
            if i < chars.len() && chars[i] == ',' { i += 1; }
        }
        Ok(out)
    }

    fn parse_entry_list(entry_src: &str) -> Result<PatchSet> {
        // entry_src like: [ ('hex', off), 'repl' ] OR [ [ ('hex',off), (...) ], 'repl'? ]
        let inner = &entry_src[1..entry_src.len()-1];
        let parts = split_top_level(inner, ',');
        if parts.is_empty() { return Err(anyhow::anyhow!("empty entry")); }
        let first = parts[0].trim();
        let mut default_repl = None;
        if parts.len() >= 2 {
            let p1 = parts[1].trim();
            if p1.starts_with('\'') { default_repl = Some(unquote(p1)?); }
        }
        if first.starts_with('[') {
            let patterns = parse_patterns_list(first)?;
            Ok(PatchSet { patterns, default_replacement: default_repl })
        } else if first.starts_with('(') {
            let pat = parse_tuple_pattern(first)?;
            Ok(PatchSet { patterns: vec![pat], default_replacement: default_repl })
        } else {
            Err(anyhow::anyhow!("entry must start with [ or ("))
        }
    }

    fn parse_patterns_list(src: &str) -> Result<Vec<PatternSpec>> {
        // src: like [ ('hex', off[, 'override']), ... ]
        let inner = &src[1..src.len()-1];
        let mut i = 0usize; let chars: Vec<char> = inner.chars().collect(); let mut out = Vec::new();
        while i < chars.len() {
            while i < chars.len() && chars[i].is_whitespace() { i += 1; }
            if i >= chars.len() { break; }
            if chars[i] != '(' { return Err(anyhow::anyhow!("expected tuple")); }
            let mut depth = 0i32; let start = i; while i < chars.len() { let c = chars[i]; if c == '(' { depth += 1; } if c == ')' { depth -= 1; if depth == 0 { i += 1; break; } } i += 1; }
            let tup = chars[start..i].iter().collect::<String>();
            out.push(parse_tuple_pattern(&tup)?);
            while i < chars.len() && chars[i] != '(' { if chars[i] == ',' { i += 1; break; } i += 1; }
        }
        Ok(out)
    }

    fn parse_tuple_pattern(src: &str) -> Result<PatternSpec> {
        // src: ('hex', off[, 'override'])
        let inner = &src[1..src.len()-1];
        let parts = split_top_level(inner, ',');
        if parts.len() < 2 { return Err(anyhow::anyhow!("tuple too short")); }
        let hex = unquote(parts[0].trim())?;
        let offset: isize = parts[1].trim().parse().unwrap_or(0);
        let override_hex = if parts.len() >= 3 { Some(unquote(parts[2].trim())?) } else { None };
        Ok(PatternSpec { hex_mask: hex, offset, override_hex })
    }

    fn parse_string(chars: &[char], mut i: usize) -> Result<(String, usize)> {
        if chars[i] != '\'' { return Err(anyhow::anyhow!("expected string")); }
        i += 1; let start = i; while i < chars.len() && chars[i] != '\'' { i += 1; }
        let s = chars[start..i].iter().collect::<String>();
        Ok((s, i+1))
    }

    fn split_top_level(s: &str, delim: char) -> Vec<String> {
        let mut res = Vec::new(); let mut depth = 0i32; let mut cur = String::new();
        for c in s.chars() {
            match c { '[' | '(' | '{' => { depth += 1; cur.push(c); }, ']' | ')' | '}' => { depth -= 1; cur.push(c); }, d if d == delim && depth == 0 => { res.push(cur.trim().to_string()); cur.clear(); }, _ => cur.push(c) }
        }
        if !cur.trim().is_empty() { res.push(cur.trim().to_string()); }
        res
    }

    fn unquote(s: &str) -> Result<String> { Ok(s.trim_matches('\'').to_string()) }

    let d32 = find_dict("patches32").or_else(|_| find_dict("patches_32")).unwrap_or("{}");
    let d64 = find_dict("patches64").or_else(|_| find_dict("patches_64")).unwrap_or("{}");
    Ok((parse_dict(d32)?, parse_dict(d64)?))
}

fn findmask(data: &[u8], hex_mask: &str, mut start: usize) -> Option<usize> {
    // Python-compatible masked search with '??' as single-byte wildcard.
    if !hex_mask.contains("??") {
        let needle = hex::decode(hex_mask).ok()?;
        return twoway::find_bytes(&data[start..], &needle).map(|p| start + p);
    }
    let parts: Vec<&str> = hex_mask.split("??").collect();
    loop {
        let anchor = hex::decode(parts[0]).ok()?;
        let findpos = twoway::find_bytes(&data[start..], &anchor).map(|p| start + p)?;
        let mut good = true;
        let mut checkpos = findpos;
        for part in &parts {
            if !part.is_empty() {
                let b = hex::decode(part).ok()?;
                if checkpos + b.len() > data.len() || &data[checkpos..checkpos + b.len()] != b.as_slice() { good = false; break; }
            }
            checkpos += (part.len() / 2) + 1; // advance past this literal and one wildcard byte
        }
        if good { return Some(findpos); }
        start = findpos + 1;
    }
}

fn apply_patchsets_to_file(orig: &[u8], out: &mut [u8], sets: &[PatchSet], warnings: &mut Vec<String>) {
    for set in sets {
        // Choose first matching pattern with exactly one match
        let mut chosen: Option<(usize, &PatternSpec)> = None;
        for pat in &set.patterns {
            let p1 = findmask(orig, &pat.hex_mask, 0);
            let p2 = p1.and_then(|p| findmask(orig, &pat.hex_mask, p+1));
            if let Some(pos) = p1 { if p2.is_none() { chosen = Some((pos, pat)); break; } }
        }
        if let Some((base, pat)) = chosen {
            let repl_hex = pat.override_hex.as_ref().or(set.default_replacement.as_ref());
            if let Some(hexs) = repl_hex {
                if let Ok(repl) = hex::decode(hexs) {
                    let off = if pat.offset >= 0 { (base as isize + pat.offset) as usize } else { base.saturating_sub(pat.offset.unsigned_abs()) };
                    if off + repl.len() <= out.len() {
                        out[off..off+repl.len()].copy_from_slice(&repl);
                        // Log applied patch summary as a warning entry (UI prints these now)
                        warnings.push(format!("Applied patch at 0x{:X}, len {}", off, repl.len()));
                    } else {
                        warnings.push(format!("Write out of range for pattern {}", pat.hex_mask));
                    }
                }
            }
        } else {
            // Log candidate locations for diagnostics
            let mut locs: Vec<String> = Vec::new();
            for pat in &set.patterns {
                let mut start = 0usize;
                while let Some(p) = findmask(orig, &pat.hex_mask, start) { locs.push(format!("{}@0x{:X}", &pat.hex_mask, p)); start = p + 1; }
            }
            if !locs.is_empty() {
                warnings.push(format!("Ambiguous or conflicting pattern(s): {}", locs.join(", ")));
            } else {
                warnings.push("Failed to locate pattern".to_string());
            }
        }
    }
}

fn write_patched_file(dest_root: &Path, rel_path: &str, content: &[u8]) -> Result<()> {
    let out = dest_root.join("patched").join(rel_path);
    if let Some(parent) = out.parent() { std::fs::create_dir_all(parent).ok(); }
    std::fs::write(out, content).context("write patched file")
}

pub async fn apply_patches_from_repo(owner: &str, repo: &str, file_path: &str, rtx_root: &Path, mut progress: impl FnMut(&str, u8)) -> Result<PatchResult> {
    progress("Fetching patch script", 5);
    // Try default branch path first, then a simple fallback if the repo uses master
    let url = format!("https://raw.githubusercontent.com/{}/{}/refs/heads/main/{}", owner, repo, file_path);
    let client = Client::new();
    let resp = client.get(&url).header("User-Agent", "RTXLauncher-RS").send().await?;
    let text = if resp.status().is_success() {
        resp.text().await?
    } else {
        let alt = format!("https://raw.githubusercontent.com/{}/{}/master/{}", owner, repo, file_path);
        client.get(&alt).header("User-Agent", "RTXLauncher-RS").send().await?.error_for_status()?.text().await?
    };

    progress("Parsing patch definitions", 10);
    let (map32, map64) = parse_patches_from_python(&text)?;

    // Determine 32/64 via existing detection: prefer explicit win64 presence
    let is64 = rtx_root.join("bin").join("win64").exists();
    let map = if is64 { &map64 } else { &map32 };

    let mut warnings: Vec<String> = Vec::new();
    let mut files_patched = 0usize;
    let mut patched_files: Vec<String> = Vec::new();
    let keys: Vec<String> = map.keys().cloned().collect();
    let total = keys.len().max(1);
    for (i, rel) in keys.iter().enumerate() {
        let pct = 12 + ((i as f32 / total as f32) * 80.0) as u8;
        progress(&format!("Patching {}", rel), pct.min(90));
        // Force 64-bit targets if this is a 64-bit install: rewrite known 32-bit DLL keys to win64 equivalents
        let effective_rel = if is64 && rel.starts_with("bin/") && !rel.contains("/win64/") && rel.ends_with(".dll") {
            // Upgrade to win64 path when appropriate (e.g., bin/engine.dll -> bin/win64/engine.dll)
            let tail = rel.trim_start_matches("bin/");
            format!("bin/win64/{}", tail)
        } else { rel.clone() };
        // Prefer vanilla game's DLLs (from Steam install) as source when available
        let vanilla_root = crate::steam::detect_gmod_install_folder().unwrap_or_else(|| rtx_root.to_path_buf());
        let path = vanilla_root.join(&effective_rel);
        if !path.exists() {
            // Try client.dll search behavior if needed
            if effective_rel.ends_with("bin/client.dll") {
                if let Ok(entries) = std::fs::read_dir(rtx_root) {
                    let mut found = None;
                    for ent in entries.flatten() {
                        let try_p = ent.path().join(&effective_rel);
                        if try_p.exists() { found = Some(try_p); break; }
                    }
                    if let Some(p) = found { patch_file(&p, &effective_rel, &map[rel], rtx_root, &mut warnings, &mut files_patched)?; continue; }
                }
            }
            warnings.push(format!("Missing file [{}]", effective_rel));
            continue;
        }
        patch_file(&path, &effective_rel, &map[rel], rtx_root, &mut warnings, &mut files_patched)?;
        patched_files.push(effective_rel);
    }

    progress("Writing outputs", 95);
    // Deploy patched files to live bin/bin/win64
    progress("Deploying patched files", 97);
    for rel in &patched_files {
        let src = rtx_root.join("patched").join(rel);
        let dst = rtx_root.join(rel);
        if let Some(parent) = dst.parent() { let _ = std::fs::create_dir_all(parent); }
        if let Err(e) = std::fs::copy(&src, &dst) { warnings.push(format!("Failed to deploy {}: {}", rel, e)); }
    }
    
    progress("Writing report", 98);
    // Write a report next to outputs for debugging
    if let Some(report_dir) = std::path::Path::new(rtx_root).join("patched").to_str().map(|s| s.to_string()) {
        let report_path = std::path::Path::new(&report_dir).join("patch-report.txt");
        let mut text = String::new();
        text.push_str(&format!("Patched {} file(s)\n", files_patched));
        for f in &patched_files { text.push_str(&format!("Patched: {}\n", f)); }
        for w in &warnings { text.push_str(&format!("{}\n", w)); }
        let _ = std::fs::create_dir_all(std::path::Path::new(&report_dir));
        let _ = std::fs::write(&report_path, text);
    }
    progress("Done", 100);
    Ok(PatchResult { files_patched, warnings })
}

fn patch_file(path: &Path, rel: &str, sets: &[PatchSet], install_dir: &Path, warnings: &mut Vec<String>, files_patched: &mut usize) -> Result<()> {
    let data = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let mut out = data.clone();
    apply_patchsets_to_file(&data, &mut out, sets, warnings);
    write_patched_file(install_dir, rel, &out)?;
    *files_patched += 1;
    Ok(())
}


