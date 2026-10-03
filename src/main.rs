//! Pocai — AI in your terminal.
//!
//! CLI entry point: dependency check, argument parsing, and dispatch
//! into normal / file / chat mode. Everything else lives in its own
//! module — see the `mod` declarations below.

mod api;
mod config;
mod consts;
mod context;
mod http;
mod md;
mod modes;
mod render;
mod tools;
mod ui;

use std::env;

use api::AppState;
use config::{load_api_key, load_config, save_api_key};
use consts::DEFAULT_MODEL;
use modes::{chat_mode, file_mode, normal_mode};
use ui::{error, print_usage_error, show_help, show_version};

/// HTTP, JSON, and Markdown rendering are all handled in-process, so
/// there are no external dependencies to check for at startup.
fn main() {
    let raw_args: Vec<String> = env::args().skip(1).collect();
    if raw_args.is_empty() {
        show_help();
        return;
    }

    let mut chat = false;
    let mut file = String::new();
    let mut custom_model = String::new();
    let mut rest: Vec<String> = Vec::new();
    let mut i = 0usize;

    while i < raw_args.len() {
        match raw_args[i].as_str() {
            "-k" | "--key" => {
                let key = raw_args.get(i + 1).cloned().unwrap_or_default();
                if key.is_empty() {
                    error("Missing API key.");
                    std::process::exit(1);
                }
                let cfg = load_config();
                let model = if cfg.saved_model.is_empty() {
                    DEFAULT_MODEL.to_string()
                } else {
                    cfg.saved_model
                };
                save_api_key(&key, &model);
                return;
            }
            "-c" | "--chat" => {
                chat = true;
                i += 1;
            }
            "-f" | "--file" => {
                let f = raw_args.get(i + 1).cloned().unwrap_or_default();
                if f.is_empty() {
                    error("Missing file.");
                    std::process::exit(1);
                }
                file = f;
                i += 2;
            }
            "-m" | "--model" => {
                let m = raw_args.get(i + 1).cloned().unwrap_or_default();
                if m.is_empty() {
                    error("Missing model.");
                    std::process::exit(1);
                }
                custom_model = m;
                i += 2;
            }
            "-h" | "--help" => {
                show_help();
                return;
            }
            "-v" | "--version" => {
                show_version();
                return;
            }
            s if s.starts_with('-') => {
                error(&format!("Unknown option: {}", s));
                print_usage_error("");
                std::process::exit(1);
            }
            _ => {
                rest = raw_args[i..].to_vec();
                break;
            }
        }
    }

    let cfg = load_api_key();
    let model = if !custom_model.is_empty() {
        custom_model
    } else if !cfg.saved_model.is_empty() {
        cfg.saved_model
    } else {
        DEFAULT_MODEL.to_string()
    };

    let mut state = AppState::new(cfg.api_key, model);

    if chat {
        chat_mode(&mut state);
        return;
    }

    if !file.is_empty() {
        file_mode(&mut state, &file, &rest.join(" "));
        return;
    }

    if rest.is_empty() {
        error("No prompt provided.");
        std::process::exit(1);
    }
    normal_mode(&mut state, &rest.join(" "));
}
