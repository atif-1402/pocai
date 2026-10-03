//! Git status/diff tools.

use std::process::{Command, Stdio};

use crate::context::run_capture;

fn inside_git_repo() -> bool {
    Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn tool_git_status() -> String {
    if !inside_git_repo() {
        return String::from("Not a Git repository.");
    }
    run_capture("git", &["status", "--short"]).unwrap_or_default()
}

pub fn tool_git_diff() -> String {
    if !inside_git_repo() {
        return String::from("Not a Git repository.");
    }
    run_capture("git", &["diff", "--", ".", ":!.git"]).unwrap_or_default()
}
