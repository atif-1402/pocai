//! OpenRouter chat-completions client and the tool-calling loop.

use serde_json::{json, Value};

use crate::consts::{API_URL, MAX_TOOL_ITERATIONS};
use crate::http::http_client;
use crate::render::{render_response, run_with_spinner};
use crate::tools::{build_tools, execute_tool};
use crate::ui::{error, print_pocai};

pub struct AppState {
    pub api_key: String,
    pub model: String,
    pub conversation: Vec<Value>,
}

impl AppState {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            api_key,
            model,
            conversation: Vec::new(),
        }
    }
}

fn build_system_prompt(context: &str) -> String {
    format!(
        r#"You are Pocai, an AI assistant running inside the user's terminal.

The user name is Atif and he is from Bhopal, Madhya Pradesh, India.

Be useful, conversational, concise, technically accurate and don't use emojies.

============================================================
TERMINAL AND WEB ACCESS
============================================================

You have access to the user's actual terminal through tools.

SAFE READ-ONLY TOOLS:

- get_current_time
- web_search
- get_weather
- fetch_url
- get_current_directory
- get_direct_file_count
- get_recursive_file_count
- list_directory
- read_file
- find_files
- search_files
- get_git_status
- get_git_diff

These operations do not require approval.

run_command is different.

run_command executes an actual shell command and ALWAYS requires
explicit user approval.

============================================================
WEB RULES
============================================================

These rules are MANDATORY, not suggestions. You must never answer
a question about current, local, or real-world information from
memory alone when a tool can get the real answer.

Use get_current_time when the user asks for the current time,
date, day, timezone, or anything that depends on now.

Use get_weather whenever weather, temperature, rain, forecast, or
conditions are asked about, EVEN INDIRECTLY (e.g. "should I carry
an umbrella", "what's it like outside", "how's the weather at
home"). Never estimate or guess weather from seasonal/general
knowledge. If the user does not name a location, use their known
home location. Call get_weather again if the user asks about a
different location.

Use web_search for anything current, recent, local, or that may
have changed: news, events, "what's going on", prices, sports
results, public webpages, local happenings, or facts you are not
fully certain are still true. Vague queries like "what's going on
in my area" or "any news today" still require at least one
web_search call (e.g. combine the user's known location with
"news" or "events") before you answer — do not fall back to
generic knowledge just because the query is broad. If the first
search is too broad or unhelpful, narrow the query and search
again rather than giving up.

Use fetch_url when the user gives a URL or when web_search returns
a result that should be opened/read before answering.

Never claim you checked the web, weather, or news unless you
actually called web_search, get_weather, or fetch_url in this
turn.

If a tool call fails or returns no usable results, say so plainly
and explicitly (e.g. "the web search failed" / "I couldn't find a
location match"), then answer from general knowledge only if
appropriate — do not silently substitute a guess and present it as
current information.

============================================================
TOOL RULES
============================================================

Always prefer dedicated read-only tools.

Examples:

"What time is it?"
→ get_current_time

"Search the web for latest Node.js version"
→ web_search

"Summarize https://example.com"
→ fetch_url

"What files are here?"
→ list_directory

"Read this script."
→ read_file

"Find files containing config."
→ search_files

"What is my current directory?"
→ get_current_directory

"Show Git status."
→ get_git_status

Do NOT use run_command for these.

Use run_command only when execution or modification is actually
required.

============================================================
GENERIC QUESTIONS
============================================================

If the user asks for a generic example, tutorial, explanation,
or sample code, DO NOT inspect their filesystem.

For example:

"show me a simple Bash script"

should generate a new example.

It should NOT search the user's files.

Only inspect the filesystem when the user asks about their actual
files, project, directory, scripts, or code.

============================================================
ACCURACY
============================================================

Never guess what a file does based only on its filename.

If you need to know what a file does, read it.

Never claim a command was executed unless it actually was.

Never invent terminal output.

For current or changing facts, use web_search or fetch_url.

============================================================
CODE
============================================================

When generating code:

- Make it syntactically valid.
- Use the correct language.
- Put code inside fenced Markdown code blocks.
- Always specify the language.

Example:

```bash
echo "Hello"
```

============================================================
STYLE
============================================================

Keep responses concise unless the user asks for detail.

Markdown is allowed.

Use proper Markdown tables when useful.

Do not expose internal tool JSON.

CURRENT TERMINAL CONTEXT:

{context}"#
    )
}

fn api_request(
    api_key: &str,
    model: &str,
    messages: &[Value],
    tools: &Value,
) -> Result<String, String> {
    let payload = json!({
        "model": model,
        "messages": messages,
        "tools": tools,
        "tool_choice": "auto",
    });
    let client = http_client();
    let resp = client
        .post(API_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .header("X-Title", "Pocai")
        .json(&payload)
        .send()
        .map_err(|e| format!("OpenRouter request failed: {}", e))?;
    let status = resp.status();
    let body = resp
        .text()
        .map_err(|e| format!("OpenRouter request failed: {}", e))?;
    if !status.is_success() {
        if let Ok(v) = serde_json::from_str::<Value>(&body) {
            if let Some(msg) = v.pointer("/error/message").and_then(|m| m.as_str()) {
                return Err(msg.to_string());
            }
        }
        return Err(format!(
            "OpenRouter request failed (HTTP {}): {}",
            status,
            body.chars().take(500).collect::<String>()
        ));
    }
    Ok(body)
}

pub fn ask_ai(state: &mut AppState, prompt: &str, context: &str) -> Result<(), ()> {
    let system_prompt = build_system_prompt(context);
    let mut messages: Vec<Value> = if state.conversation.is_empty() {
        vec![
            json!({"role": "system", "content": system_prompt}),
            json!({"role": "user", "content": prompt}),
        ]
    } else {
        let mut m = state.conversation.clone();
        m.push(json!({"role": "user", "content": prompt}));
        m
    };

    let tools = build_tools();

    for _ in 0..MAX_TOOL_ITERATIONS {
        let api_key = state.api_key.clone();
        let model = state.model.clone();
        let msgs = messages.clone();
        let tls = tools.clone();

        let result: Result<String, String> =
            run_with_spinner(move || api_request(&api_key, &model, &msgs, &tls));

        let body = match result {
            Ok(b) => b,
            Err(e) => {
                error(&e);
                return Err(());
            }
        };

        let parsed: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
        if let Some(msg) = parsed.pointer("/error/message").and_then(|m| m.as_str()) {
            error(msg);
            return Err(());
        }

        let assistant = parsed
            .pointer("/choices/0/message")
            .cloned()
            .unwrap_or(Value::Null);
        if assistant.is_null() {
            error("OpenRouter returned no assistant message.");
            return Err(());
        }

        messages.push(assistant.clone());

        let tool_count = assistant
            .get("tool_calls")
            .and_then(|t| t.as_array())
            .map(|a| a.len())
            .unwrap_or(0);

        if tool_count == 0 {
            let answer = assistant
                .get("content")
                .and_then(|c| c.as_str())
                .unwrap_or("")
                .to_string();
            if answer.trim().is_empty() {
                error("The model returned an empty response.");
                return Err(());
            }
            print_pocai();
            render_response(&answer);
            state.conversation = messages;
            return Ok(());
        }

        for i in 0..tool_count {
            let call = assistant
                .get("tool_calls")
                .and_then(|t| t.get(i))
                .cloned()
                .unwrap_or(Value::Null);
            let tool_id = call
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let function_name = call
                .pointer("/function/name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let args_raw = call
                .pointer("/function/arguments")
                .map(|v| {
                    if v.is_string() {
                        v.as_str().unwrap_or("{}").to_string()
                    } else {
                        v.to_string()
                    }
                })
                .unwrap_or_else(|| String::from("{}"));
            let args: Value = serde_json::from_str(&args_raw).unwrap_or_else(|_| json!({}));
            let tool_result = execute_tool(&function_name, &args);
            messages.push(json!({
                "role": "tool",
                "tool_call_id": tool_id,
                "content": tool_result,
            }));
        }
    }

    error("Pocai reached the maximum tool-call limit.");
    Err(())
}
