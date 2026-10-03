//! Config file handling: save/load the API key and model, using
//! shell-quote-compatible escaping so the file stays a plain,
//! human-editable `KEY=value` text file.

use std::env;
use std::fs;
use std::path::PathBuf;

use crate::ui::{error, palette};

pub fn config_dir() -> PathBuf {
    if let Some(home) = dirs::home_dir() {
        home.join(".config").join("pocai")
    } else {
        PathBuf::from(".config/pocai")
    }
}

pub fn config_file() -> PathBuf {
    config_dir().join("config")
}

/// Quote a value the same way POSIX shell `printf '%q'` does for
/// simple cases, so the config file stays diffable/editable by hand.
pub fn shell_quote(s: &str) -> String {
    if s.is_empty() {
        return String::from("''");
    }
    let safe = s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | ':' | ','));
    if safe {
        return s.to_string();
    }
    let mut out = String::from("'");
    for c in s.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

/// Unquote a value: handles '...', "...", $'...' and backslash
/// escapes well enough for API keys / model names.
pub fn shell_unquote(raw: &str) -> String {
    let s = raw.trim();
    if s.len() >= 2 && s.starts_with('\'') && s.ends_with('\'') {
        return s[1..s.len() - 1].replace("'\\''", "'");
    }
    if s.len() >= 3 && s.starts_with("$'") && s.ends_with('\'') {
        let inner = &s[2..s.len() - 1];
        let mut out = String::new();
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('r') => out.push('\r'),
                    Some('\\') => out.push('\\'),
                    Some('\'') => out.push('\''),
                    Some('\"') => out.push('\"'),
                    Some(other) => {
                        out.push('\\');
                        out.push(other);
                    }
                    None => out.push('\\'),
                }
            } else {
                out.push(c);
            }
        }
        return out;
    }
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        let inner = &s[1..s.len() - 1];
        let mut out = String::new();
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('\"') => out.push('\"'),
                    Some('\\') => out.push('\\'),
                    Some('`') => out.push('`'),
                    Some('$') => out.push('$'),
                    Some('n') => out.push('\n'),
                    Some(other) => {
                        out.push('\\');
                        out.push(other);
                    }
                    None => out.push('\\'),
                }
            } else {
                out.push(c);
            }
        }
        return out;
    }
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(n) = chars.next() {
                out.push(n);
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub fn save_api_key(key: &str, model: &str) {
    let p = palette();
    if key.is_empty() {
        error("API key cannot be empty.");
        std::process::exit(1);
    }
    let dir = config_dir();
    if let Err(e) = fs::create_dir_all(&dir) {
        error(&format!("Could not create config dir: {}", e));
        std::process::exit(1);
    }
    let file = config_file();
    let content = format!(
        "API_KEY={}\nMODEL={}\n",
        shell_quote(key),
        shell_quote(model)
    );
    if let Err(e) = fs::write(&file, content) {
        error(&format!("Could not write config: {}", e));
        std::process::exit(1);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&file, fs::Permissions::from_mode(0o600));
    }
    println!();
    println!("{}Pocai:{} API key saved.", p.green, p.reset);
    println!("{}Config:{} {}", p.dim, p.reset, file.display());
    println!();
}

pub struct LoadedConfig {
    pub api_key: String,
    pub saved_model: String,
}

pub fn load_config() -> LoadedConfig {
    let mut api_key = String::new();
    let mut saved_model = String::new();
    let file = config_file();
    if let Ok(text) = fs::read_to_string(&file) {
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(rest) = line.strip_prefix("API_KEY=") {
                api_key = shell_unquote(rest);
            } else if let Some(rest) = line.strip_prefix("MODEL=") {
                saved_model = shell_unquote(rest);
            }
        }
    }
    LoadedConfig {
        api_key,
        saved_model,
    }
}

pub fn load_api_key() -> LoadedConfig {
    let mut cfg = load_config();
    if let Ok(env_key) = env::var("OPENROUTER_API_KEY") {
        if !env_key.is_empty() {
            cfg.api_key = env_key;
        }
    }
    if cfg.api_key.is_empty() {
        error("OpenRouter API key is not configured.");
        println!();
        println!("Run:");
        println!();
        println!("  pocai -k \"YOUR_OPENROUTER_API_KEY\"");
        println!();
        std::process::exit(1);
    }
    cfg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quote_roundtrip() {
        for s in ["", "abc", "sk-or-v1-abc", "hello world", "it's", "a$b"] {
            let q = shell_quote(s);
            assert_eq!(shell_unquote(&q), s, "roundtrip failed for {:?}", s);
        }
    }
}
