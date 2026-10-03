# Pocai

Pocai is a lightweight terminal AI agent built in Rust. It runs locally, understands your shell context, can read files, search the repo, inspect git state, fetch URLs, and render rich markdown directly in the terminal.

It is designed to feel fast, minimal, and terminal-native rather than bloated or web-app-like.

## Why Pocai?

Pocai is for people who want AI in the terminal without:
- a heavy desktop app
- Python runtime overhead
- external markdown renderers
- noisy UI and browser-like workflows

Instead, it gives you:
- a fast Rust binary
- terminal-aware tool usage
- built-in markdown rendering
- syntax-highlighted code blocks
- tables, math, diagrams, and rich output in plain terminal output
- a minimal-but-capable agent workflow

## Features

- Terminal-native AI chat
- Markdown rendering in-process. The layout is inspired by
  [`leaf`](https://github.com/RivoLink/leaf), which Pocai used to shell
  out to. Thank you to RivoLink for the original design.
- Syntax highlighting for code blocks
- Tables, lists, blockquotes, and formatted output
- Math rendering with LaTeX-style input
- Mermaid diagram rendering
- File and directory awareness
- Git-aware tools
- URL fetching
- Web search support
- Shell access with approval-gated execution
- Local configuration for API keys

## Requirements

- Rust toolchain
- An OpenRouter API key or compatible model provider

## Installation

Clone the repository:

```bash
git clone https://github.com/atif-1402/pocai.git
cd pocai
cargo build --release
```

Then copy the binary to a location in your PATH:

```bash
cp target/release/pocai ~/.local/bin/
```

Or run it directly from the build directory:

```bash
./target/release/pocai
```

## Configuration

Set your API key in the environment:

```bash
export OPENROUTER_API_KEY="your-key-here"
```

Or save it with Pocai:

```bash
pocai -k "your-key-here"
```

This stores the key in your local config directory.

## Usage

Basic prompt:

```bash
pocai "what time is it?"
```

Ask it to inspect your project:

```bash
pocai "explain this project"
```

Inspect git state:

```bash
pocai "show me git status"
```

Search files:

```bash
pocai "find the main config file for this project"
```

Summarize a URL:

```bash
pocai "summarize https://example.com"
```

Open interactive chat mode:

```bash
pocai -c
```

Run a file-based session:

```bash
pocai -f script.sh
pocai -f script.sh "find bugs"
```

Choose a model:

```bash
pocai -m openrouter/free "hello"
```

Show help:

```bash
pocai -h
```

## Example

```bash
$ pocai "show me git status"

On branch main
Your branch is up to date with 'origin/main'.

Changes not staged for commit:
  modified: src/main.rs
  modified: README.md
```

Pocai renders this using its built-in formatting engine so it stays readable in the terminal.

## Project Layout

```text
src/
├── main.rs
├── consts.rs
├── config.rs
├── context.rs
├── http.rs
├── ui.rs
├── render.rs
├── md/
│   ├── mod.rs
│   ├── style.rs
│   ├── ansi.rs
│   ├── width.rs
│   ├── wrap.rs
│   ├── inline.rs
│   ├── blocks.rs
│   ├── lists.rs
│   ├── code.rs
│   ├── highlight.rs
│   ├── links.rs
│   ├── table.rs
│   ├── latex.rs
│   ├── mermaid.rs
│   └── smart.rs
├── api.rs
├── modes.rs
└── tools/
    ├── mod.rs
    ├── fs.rs
    ├── git.rs
    ├── web.rs
    ├── weather.rs
    └── shell.rs
```

## Notes

This project is currently in active development. The terminal renderer and tool loop are the main focus, and more polish is still being added around UX, model behavior, and tooling.

## License

MIT

## Acknowledgements

This project uses in-process markdown rendering and terminal layout techniques inspired by terminal-first interfaces and rich CLI experiences.
