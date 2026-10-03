# Pocai

AI in your terminal — tool-calling, terminal-aware, Markdown-rendered
in-process, written in Rust.

## Build

```
cargo build --release
```

The binary is at `target/release/pocai`. Copy it somewhere on your
`PATH`, e.g. `~/.local/bin/pocai`.

## Requirements

- Nothing beyond a Rust toolchain. Markdown rendering, syntax
  highlighting, and diagram layout all run in-process.
- `git` and `ripgrep` (or `grep`) — optional; used by the `get_git_*`
  and `search_files` tools when available.

## Rendering

Responses are rendered by the built-in `md` module: a pull-down-CMark
event loop that emits styled lines, with no subprocess and nothing to
install. It handles tables, line-numbered and syntax-highlighted code,
LaTeX, Mermaid flowcharts and sequence diagrams, smart punctuation, and
bare-URL linkification. Color is emitted only when stdout is a terminal
and `NO_COLOR` is unset, so `pocai "..." > out.txt` stays clean text.

The layout is inspired by
[`leaf`](https://github.com/RivoLink/leaf), which Pocai used to shell
out to. Thank you to RivoLink for the original design.

## Setup

```
pocai -k "sk-or-v1-..."
```

Saves your OpenRouter API key to `~/.config/pocai/config` (mode
`600`). You can set `OPENROUTER_API_KEY` in your environment instead
if you'd rather not save it to disk.

## Usage

```
pocai "what time is it?"
pocai "search the web for latest Linux kernel release"
pocai "summarize https://example.com"
pocai "what directory am I in?"
pocai "explain this project"
pocai "show me git status"

pocai -c

pocai -f script.sh
pocai -f script.sh "find bugs"

pocai -k "sk-or-v1-..."
pocai -m openrouter/free "hello"

pocai -h
pocai -v
```

## Project layout

```
src/
├── main.rs        CLI entry point, argument parsing, dispatch
├── consts.rs       shared constants
├── config.rs       API key / model config load & save
├── context.rs      cwd/OS/kernel/shell snapshot for the system prompt
├── http.rs         shared HTTP client
├── ui.rs           colours, prompts, banners, /command router
├── render.rs        spinner + terminal width/color detection
├── md/              the in-process Markdown renderer
│   ├── mod.rs         event loop: Markdown -> styled lines
│   ├── style.rs       Run/Line/Style, the ANSI-16 color model
│   ├── ansi.rs        SGI output, width-safe, style-caching
│   ├── width.rs       grapheme and display-width helpers
│   ├── wrap.rs        width-aware wrapping, prefixes, hard splits
│   ├── inline.rs      inline spans, emphasis, code, math
│   ├── blocks.rs      headings, rules, quotes, paragraphs
│   ├── lists.rs       bullets, numbering, nesting, task lists
│   ├── code.rs        line-numbered code frames
│   ├── highlight.rs   syntect -> ANSI-16 quantization
│   ├── links.rs       link destinations, bare-URL linkification
│   ├── table.rs       column sizing, alignment, box chrome
│   ├── latex.rs       LaTeX -> Unicode
│   ├── mermaid.rs     flowchart and sequence diagrams
│   └── smart.rs       smart punctuation
├── api.rs          OpenRouter client + the tool-calling loop
├── modes.rs         normal / file / chat mode
└── tools/
    ├── mod.rs        tool schema + dispatcher
    ├── fs.rs         list_directory, read_file, find_files, search_files
    ├── git.rs        get_git_status, get_git_diff
    ├── web.rs        fetch_url, web_search
    ├── weather.rs    get_weather
    └── shell.rs      run_command (approval-gated)
```
# pocai
