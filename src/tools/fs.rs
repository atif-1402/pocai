//! Filesystem-inspection tools: directory listing, file reading,
//! filename search, and content search.

use std::fs;
use std::path::Path;
use std::process::Command;

use walkdir::WalkDir;

use crate::context::entry_type_char;

pub fn tool_list_directory(dir: &str) -> String {
    let d = if dir.is_empty() { "." } else { dir };
    let path = Path::new(d);
    if !path.is_dir() {
        return format!("Directory does not exist: {}", d);
    }
    let mut lines: Vec<String> = Vec::new();
    for entry in WalkDir::new(path)
        .max_depth(2)
        .into_iter()
        .filter_entry(|e| {
            if e.depth() == 0 {
                return true;
            }
            let p = e.path().to_string_lossy().to_string();
            !(p.contains("/.git") || p.ends_with("/.git"))
        })
        .filter_map(|e| e.ok())
    {
        lines.push(format!(
            "{} {}",
            entry_type_char(entry.path()),
            entry.path().display()
        ));
        if lines.len() >= 300 {
            break;
        }
    }
    lines.join("\n")
}

fn looks_binary_bytes(bytes: &[u8]) -> bool {
    if bytes.contains(&0) {
        return true;
    }
    let sample = &bytes[..bytes.len().min(8000)];
    if sample.is_empty() {
        return false;
    }
    let odd = sample
        .iter()
        .filter(|b| **b < 9 || (**b > 13 && **b < 32) || **b == 127)
        .count();
    odd * 100 / sample.len() > 10
}

pub fn tool_read_file(file: &str) -> String {
    let path = Path::new(file);
    if !path.is_file() {
        return format!("File does not exist: {}", file);
    }
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => return format!("Could not read file: {}", e),
    };
    let binary_ext = matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some(
            "png"
                | "jpg"
                | "jpeg"
                | "gif"
                | "webp"
                | "mp4"
                | "mp3"
                | "wav"
                | "ogg"
                | "zip"
                | "tar"
                | "gz"
                | "bz2"
                | "xz"
                | "7z"
                | "rar"
                | "pdf"
                | "exe"
                | "bin"
                | "o"
                | "so"
                | "a"
                | "class"
                | "pyc"
        )
    );
    if binary_ext || looks_binary_bytes(&bytes) {
        return String::from("This appears to be a binary file.");
    }
    if bytes.len() > 50_000 {
        let head = &bytes[..50_000];
        format!(
            "File is {} bytes. Showing first 50,000 bytes.\n\n{}",
            bytes.len(),
            String::from_utf8_lossy(head)
        )
    } else {
        String::from_utf8_lossy(&bytes).to_string()
    }
}

pub fn tool_find_files(pattern: &str) -> String {
    if pattern.is_empty() {
        return String::from("No search pattern supplied.");
    }
    let needle = pattern.to_lowercase();
    let mut out: Vec<String> = Vec::new();
    for entry in WalkDir::new(".")
        .into_iter()
        .filter_entry(|e| {
            if e.depth() == 0 {
                return true;
            }
            let p = e.path().to_string_lossy().to_string();
            !(p == "./.git" || p.starts_with("./.git/"))
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if name.contains(&needle) {
            out.push(entry.path().display().to_string());
            if out.len() >= 300 {
                break;
            }
        }
    }
    out.join("\n")
}

pub fn tool_search_files(query: &str) -> String {
    if query.is_empty() {
        return String::from("No search query supplied.");
    }
    // Prefer ripgrep when available; fall back to grep otherwise.
    let rg = Command::new("rg")
        .args([
            "--hidden", "--glob", "!.git/**", "--glob", "!*.png", "--glob", "!*.jpg", "--glob",
            "!*.jpeg", "--glob", "!*.gif", "--glob", "!*.webp", "--glob", "!*.mp4", "--glob",
            "!*.zip", "--glob", "!*.tar*", query, ".",
        ])
        .output();
    if let Ok(o) = rg {
        if o.status.success() {
            let text = String::from_utf8_lossy(&o.stdout);
            return text.lines().take(300).collect::<Vec<_>>().join("\n");
        }
        // rg exits 1 when there are no matches — return empty like grep.
        if o.status.code() == Some(1) {
            return String::new();
        }
    }
    match Command::new("grep")
        .args(["-R", "-n", "--exclude-dir=.git", query, "."])
        .output()
    {
        Ok(o) => {
            let text = String::from_utf8_lossy(&o.stdout);
            text.lines().take(300).collect::<Vec<_>>().join("\n")
        }
        Err(e) => format!("Search failed: {}", e),
    }
}
