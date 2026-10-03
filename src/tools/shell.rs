//! `run_command`: the only tool that requires explicit user approval
//! before executing anything.

use std::io::{self, Write};
use std::process::Command;

use crate::ui::palette;

/// Cap applied to command output before it's fed back into the
/// conversation — matches the cap already used by read_file/fetch_url.
/// A runaway command (accidental `cat` of a huge file, a noisy loop)
/// shouldn't be able to blow up the request payload sent back to the
/// model. The full, untruncated output is still shown live on stderr.
const MAX_OUTPUT_BYTES: usize = 50_000;

fn confirm_command(command: &str) -> bool {
    let p = palette();
    eprintln!();
    eprintln!("{}{}Pocai wants to run:{}", p.yellow, p.bold, p.reset);
    eprintln!("  {}", command);
    eprintln!();
    eprint!("{}Allow? [y/N] {}", p.yellow, p.reset);
    let _ = io::stderr().flush();
    let mut answer = String::new();
    if io::stdin().read_line(&mut answer).is_err() {
        return false;
    }
    matches!(answer.trim(), "y" | "Y" | "yes" | "YES")
}

pub fn tool_run_command(command: &str) -> String {
    let p = palette();
    if command.is_empty() {
        return String::from("No command supplied.");
    }
    if !confirm_command(command) {
        return String::from("Command denied by user.");
    }
    eprintln!("\n{}Executing...{}\n", p.dim, p.reset);

    // Let bash itself merge stdout+stderr with `2>&1` so the combined
    // stream comes back in real chronological order. Capturing them
    // as two separate buffers and concatenating afterwards (all of
    // stdout, then all of stderr) would silently reorder any command
    // that interleaves the two streams.
    let output = Command::new("bash")
        .arg("-c")
        .arg(format!("{} 2>&1", command))
        .output();

    match output {
        Ok(o) => {
            let mut text = String::from_utf8_lossy(&o.stdout).to_string();
            if text.trim().is_empty() {
                text = String::from("(command produced no output)");
            }
            let code = o.status.code().unwrap_or(0);
            eprintln!("{}\n", text);
            eprintln!("{}Exit code: {}{}", p.dim, code, p.reset);

            if text.len() > MAX_OUTPUT_BYTES {
                let mut truncated = text[..MAX_OUTPUT_BYTES].to_string();
                truncated.push_str(&format!(
                    "\n\n[output truncated at {} bytes]",
                    MAX_OUTPUT_BYTES
                ));
                truncated
            } else {
                text
            }
        }
        Err(e) => format!("Command failed to execute: {}", e),
    }
}
