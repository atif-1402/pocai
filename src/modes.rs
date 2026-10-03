//! The three ways to invoke Pocai: a one-shot prompt, `-f <file>`
//! file analysis, and `-c` interactive chat.

use std::path::Path;
use std::process::Command;

use crate::api::{ask_ai, AppState};
use crate::context::{get_current_directory, get_terminal_context};
use crate::ui::{
    chat_separator, error, handle_chat_command, palette, print_you_prompt, separator, ChatCommand,
};

pub fn normal_mode(state: &mut AppState, prompt: &str) {
    let context = get_terminal_context();
    state.conversation.clear();
    let _ = ask_ai(state, prompt, &context);
}

pub fn file_mode(state: &mut AppState, file: &str, request: &str) {
    if !Path::new(file).is_file() {
        error(&format!("File not found: {}", file));
        std::process::exit(1);
    }
    let req = if request.trim().is_empty() {
        "Explain this file and what it does."
    } else {
        request
    };
    let prompt = format!(
        "Analyze this file:\n\n{}\n\nUser request:\n\n{}\n\nUse the read_file tool to inspect the file before answering.",
        file, req
    );
    let context = get_terminal_context();
    state.conversation.clear();
    let _ = ask_ai(state, &prompt, &context);
}

pub fn chat_mode(state: &mut AppState) {
    let p = palette();
    let _ = Command::new("clear").status();
    let current_directory = get_current_directory();

    println!();
    println!("{}{}", p.dim, p.red);
    println!("  ░▓███████▓░");
    println!(" ▒▓███ ███ █▓▒");
    println!("  ░▓███████▓░");
    println!("   ▓▊     █▓");
    print!("{}", p.reset);

    println!();
    println!(
        "{}{}Pocai{} {}— AI in your terminal{}",
        p.bold, p.cyan, p.reset, p.dim, p.reset
    );
    println!("{}Model:{} {}", p.dim, p.reset, state.model);
    crate::ui::status("Current Directory", &current_directory);
    println!("{}Terminal access:{} enabled", p.dim, p.reset);
    println!("{}Read-only access:{} enabled", p.dim, p.reset);
    println!("{}Web access:{} enabled", p.dim, p.reset);
    crate::ui::status("Type", "/help for commands \u{2022} exit / Ctrl+D to quit");
    println!();
    separator();
    println!();

    let context = get_terminal_context();
    state.conversation.clear();

    let mut rl = rustyline::DefaultEditor::new().unwrap_or_else(|e| {
        error(&format!("Could not start line editor: {}", e));
        std::process::exit(1);
    });

    loop {
        let prompt = print_you_prompt();
        let line = rl.readline(&prompt);
        let message = match line {
            Ok(l) => l.trim().to_string(),
            Err(rustyline::error::ReadlineError::Eof) => {
                println!();
                break;
            }
            Err(rustyline::error::ReadlineError::Interrupted) => continue,
            Err(e) => {
                error(&format!("Input error: {}", e));
                continue;
            }
        };

        if message == "exit" || message == "/exit" || message == "/quit" {
            println!();
            println!("{}Goodbye.{}", p.dim, p.reset);
            break;
        }
        if message.is_empty() {
            continue;
        }
        let _ = rl.add_history_entry(message.as_str());

        match handle_chat_command(&message) {
            ChatCommand::Quit => {
                println!();
                println!("{}Goodbye.{}", p.dim, p.reset);
                break;
            }
            ChatCommand::Handled => continue,
            ChatCommand::NotCommand => {}
        }

        println!();
        let _ = ask_ai(state, &message, &context);
        println!();
        chat_separator();
        println!();
    }
}
