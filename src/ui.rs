//! Small UI helpers: colours, prompts, banners, and the `/command`
//! router used by chat mode.

use std::env;
use std::io::{self, IsTerminal, Write};
use std::process::Command;
use std::sync::OnceLock;

use crate::config::config_file;
use crate::consts::VERSION;
use crate::render::render_response;

/// ANSI colour codes, auto-disabled when stdout is not a terminal
/// (redirected to a file, piped, etc.) so nothing leaks raw escape
/// bytes into non-interactive output. Computed once, lazily, since a
/// TTY check can't be a compile-time `const`.
pub struct Palette {
    pub reset: &'static str,
    pub bold: &'static str,
    pub dim: &'static str,
    pub red: &'static str,
    pub green: &'static str,
    pub yellow: &'static str,
    pub magenta: &'static str,
    pub cyan: &'static str,
}

static PALETTE: OnceLock<Palette> = OnceLock::new();

pub fn palette() -> &'static Palette {
    PALETTE.get_or_init(|| {
        if io::stdout().is_terminal() {
            Palette {
                reset: "\x1b[0m",
                bold: "\x1b[1m",
                dim: "\x1b[2m",
                red: "\x1b[31m",
                green: "\x1b[32m",
                yellow: "\x1b[33m",
                magenta: "\x1b[35m",
                cyan: "\x1b[36m",
            }
        } else {
            Palette {
                reset: "",
                bold: "",
                dim: "",
                red: "",
                green: "",
                yellow: "",
                magenta: "",
                cyan: "",
            }
        }
    })
}

/// Print a dim `label: value` status line, clipped to the terminal width.
///
/// The greeting lines carry an absolute path, which is longer than a
/// narrow terminal and wrapped mid-word past the right edge.
pub fn status(label: &str, value: &str) {
    let p = palette();
    let width = crate::render::terminal_width();
    let mut plain = format!("{label}: {value}");
    if plain.chars().count() > width {
        plain = plain
            .chars()
            .take(width.saturating_sub(1))
            .collect::<String>()
            + "\u{2026}";
    }
    // Colour the label only, so the value stays unstyled.
    let split = (label.len() + 2).min(plain.len());
    let (head, tail) = plain.split_at(split);
    println!("{}{head}{}{tail}", p.dim, p.reset);
}

pub fn separator() {
    // Shares the renderer's width source so the rule always lines up with
    // the content it separates. Resolving it twice let the two disagree:
    // a narrow `COLUMNS` produced an 80-column rule under 60-column text.
    println!("{}", "─".repeat(crate::render::terminal_width()));
}

pub fn chat_separator() {
    separator();
}

pub fn error(msg: &str) {
    let p = palette();
    eprintln!("{}Pocai:{} {}", p.red, p.reset, msg);
}

pub fn print_pocai() {
    let p = palette();
    print!("{}{}Pocai ›{} ", p.bold, p.cyan, p.reset);
    let _ = io::stdout().flush();
}

pub fn print_you_prompt() -> String {
    let p = palette();
    format!("{}{}You ›{} ", p.bold, p.magenta, p.reset)
}

// ------------------------------------------------------------
// Examples / help
// ------------------------------------------------------------

fn example_markdown() -> &'static str {
    r#"This is an example message from Pocai.
Pocai is an AI assistant that lives in your terminal.

Here's a simple table:

| Feature       | Status        |
|---------------|---------------|
| Rust          | Supported     |
| OpenRouter    | Supported     |
| Tool Calls    | Supported     |
| Web Search    | Supported     |
| Markdown      | Supported     |

Here's a simple Bash script:

```bash
#!/usr/bin/env bash

NAME="Pocai"

echo "Hello from $NAME!"

for i in 1 2 3; do
    echo "Number: $i"
done
```

And here's a simple paragraph demonstrating normal text output.
Pocai can display explanations, lists, tables, code blocks,
and other Markdown-style content directly inside your terminal.

- This is an example bullet point.
- Another example bullet point.
- Rendering is handled in-process, by Pocai itself.

You can also have numbered lists:

1. Ask Pocai a question.
2. Pocai can inspect your terminal when needed.
3. Pocai can search/fetch the web when needed.
4. Commands that execute require your approval.

**End of Pocai example.**"#
}

/// The Markdown rendering stress test, rendered by `/test`.
///
/// Kept as a Markdown file rather than a Rust string so it stays valid
/// Markdown, and so the renderer can be regression-tested against the
/// exact same input.
pub fn test_markdown() -> &'static str {
    include_str!("../testdata/render_test.md")
}

/// Render the stress test through the real output path.
pub fn show_render_test() {
    println!();
    print_pocai();
    render_response(test_markdown());
}

pub fn show_examples() {
    println!();
    print_pocai();
    render_response(example_markdown());
}

pub fn show_chat_help() {
    let p = palette();
    println!();
    println!("{}{}Interactive commands{}", p.bold, p.cyan, p.reset);
    println!();
    println!("  /example    Show a Pocai output example");
    println!("  /test       Render the Markdown stress test");
    println!("  /clear      Clear the screen");
    println!("  /pwd        Show current directory");
    println!("  /version    Show Pocai version");
    println!("  /help       Show this help");
    println!("  exit        Exit Pocai");
    println!();
}

/// `Handled` == command consumed, keep looping. `Quit` == exit the
/// chat loop. `NotCommand` == not a `/command`, pass on to the model.
pub enum ChatCommand {
    Handled,
    NotCommand,
    Quit,
}

pub fn handle_chat_command(command: &str) -> ChatCommand {
    let p = palette();
    match command {
        "/example" => {
            show_examples();
            ChatCommand::Handled
        }
        "/test" => {
            show_render_test();
            ChatCommand::Handled
        }
        "/help" => {
            show_chat_help();
            ChatCommand::Handled
        }
        "/clear" => {
            let _ = Command::new("clear").status();
            ChatCommand::Handled
        }
        "/pwd" => {
            println!();
            println!(
                "{}Current Directory:{} {}",
                p.dim,
                p.reset,
                env::current_dir()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|_| String::from("unknown"))
            );
            println!();
            ChatCommand::Handled
        }
        "/version" => {
            show_version();
            ChatCommand::Handled
        }
        "/quit" | "/exit" => ChatCommand::Quit,
        s if s.starts_with('/') => {
            error(&format!("Unknown command: {}", command));
            println!("Use {}/help{} to see available commands.", p.bold, p.reset);
            ChatCommand::Handled
        }
        _ => ChatCommand::NotCommand,
    }
}

pub fn show_help() {
    let p = palette();
    println!();
    println!("{}{}Pocai{} - AI in your terminal", p.bold, p.cyan, p.reset);
    println!();
    println!("{}Usage:{}", p.bold, p.reset);
    println!("  pocai [options] \"prompt\"");
    println!();
    println!("{}Options:{}", p.bold, p.reset);
    println!("  -c, --chat              Interactive chat");
    println!("  -f, --file <file>       Analyze a file");
    println!("  -k, --key <key>         Save OpenRouter API key");
    println!("  -m, --model <model>     Use a specific model");
    println!("  -h, --help              Show help");
    println!("  -v, --version           Show version");
    println!();
    println!("{}Examples:{}", p.bold, p.reset);
    println!("  pocai \"what time is it?\"");
    println!("  pocai \"search the web for latest Linux kernel release\"");
    println!("  pocai \"summarize https://example.com\"");
    println!("  pocai \"what directory am I in?\"");
    println!("  pocai \"how many files are here?\"");
    println!("  pocai \"explain this project\"");
    println!("  pocai \"show me git status\"");
    println!();
    println!("  pocai -c");
    println!();
    println!("  pocai -f script.sh");
    println!("  pocai -f script.sh \"find bugs\"");
    println!();
    println!("  pocai -k \"sk-or-v1-...\"");
    println!("  pocai -m openrouter/free \"hello\"");
    println!();
    println!("{}Config:{}", p.bold, p.reset);
    println!("  {}", config_file().display());
    println!();
}

pub fn show_version() {
    println!("Pocai {}", VERSION);
}

pub fn print_usage_error(extra: &str) {
    if !extra.is_empty() {
        println!("{}", extra);
    }
    println!("Run 'pocai --help' for help.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_command_routing() {
        assert!(matches!(handle_chat_command("/test"), ChatCommand::Handled));
        assert!(matches!(handle_chat_command("/help"), ChatCommand::Handled));
        assert!(matches!(handle_chat_command("/quit"), ChatCommand::Quit));
        assert!(matches!(
            handle_chat_command("hello"),
            ChatCommand::NotCommand
        ));
    }
}
