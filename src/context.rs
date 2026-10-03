//! Terminal / environment context: cwd, OS, kernel, shell, and the
//! lightweight snapshot injected into every request's system prompt.

use std::env;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use chrono::Local;

pub fn get_current_directory() -> String {
    env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| String::from("unknown"))
}

pub fn tool_current_time() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S %Z (%z)").to_string()
}

pub fn run_capture(cmd: &str, args: &[&str]) -> Option<String> {
    Command::new(cmd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8_lossy(&o.stdout).trim().to_string().into()
            } else {
                None
            }
        })
        .and_then(|s| if s.is_empty() { None } else { Some(s) })
}

pub fn os_pretty_name() -> String {
    if let Ok(text) = fs::read_to_string("/etc/os-release") {
        let mut name: Option<String> = None;
        let mut pretty: Option<String> = None;
        for line in text.lines() {
            if let Some(v) = line.strip_prefix("PRETTY_NAME=") {
                pretty = Some(crate::config::shell_unquote(v));
            } else if let Some(v) = line.strip_prefix("NAME=") {
                name = Some(crate::config::shell_unquote(v));
            }
        }
        if let Some(p) = pretty {
            if !p.is_empty() {
                return p;
            }
        }
        if let Some(n) = name {
            if !n.is_empty() {
                return n;
            }
        }
    }
    run_capture("uname", &["-s"]).unwrap_or_else(|| String::from("unknown"))
}

pub fn count_direct_files() -> usize {
    let Ok(entries) = fs::read_dir(".") else {
        return 0;
    };
    entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .count()
}

pub fn count_recursive_files() -> usize {
    walkdir::WalkDir::new(".")
        .into_iter()
        .filter_entry(|e| {
            if e.depth() == 0 {
                return true;
            }
            let name = e.file_name().to_string_lossy();
            !(name == ".git" && e.file_type().is_dir())
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .count()
}

pub fn entry_type_char(path: &Path) -> char {
    if path.is_symlink() {
        'l'
    } else if path.is_dir() {
        'd'
    } else if path.is_file() {
        'f'
    } else {
        '?'
    }
}

/// Lightweight snapshot injected into the system prompt on every
/// request. Deliberately does NOT include a directory listing, file
/// counts, or Git status — those require a recursive filesystem walk
/// and a `git` invocation, and dedicated tools already cover them on
/// demand (list_directory, get_direct_file_count,
/// get_recursive_file_count, get_git_status). Paying for a full scan
/// on every single message — including "hi" — isn't worth it.
pub fn get_terminal_context() -> String {
    let cwd = get_current_directory();
    let shell_name = env::var("SHELL")
        .ok()
        .and_then(|s| {
            Path::new(&s)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| String::from("unknown"));
    let now = Local::now().format("%Y-%m-%d %H:%M:%S %Z (%z)").to_string();
    let kernel = run_capture("uname", &["-sr"]).unwrap_or_else(|| String::from("unknown"));
    let os = os_pretty_name();

    format!(
        "Current date and time:\n{now}\n\nCurrent working directory:\n{cwd}\n\n\
         Operating system:\n{os}\n\nKernel:\n{kernel}\n\nShell:\n{shell_name}\n\n\
         For anything about files, the directory listing, file counts, or \
         Git status, call the relevant tool (list_directory, \
         get_direct_file_count, get_recursive_file_count, get_git_status) \
         instead of assuming — none of that is pre-loaded here.\n"
    )
}
