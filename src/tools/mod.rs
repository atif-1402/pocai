//! Tool schema (sent to the model) and the dispatcher that runs
//! whichever tool the model asked for.

mod fs;
mod git;
mod shell;
mod weather;
mod web;

use serde_json::{json, Value};

use crate::context::{
    count_direct_files, count_recursive_files, get_current_directory, tool_current_time,
};

pub fn build_tools() -> Value {
    json!([
        {"type": "function", "function": {"name": "get_current_time", "description": "Get the current local date and time from the terminal. Safe read-only operation.", "parameters": {"type": "object", "properties": {}}}},
        {"type": "function", "function": {"name": "web_search", "description": "Search the web for current or factual information. Safe read-only network operation.", "parameters": {"type": "object", "properties": {"query": {"type": "string", "description": "Search query."}}, "required": ["query"]}}},
        {"type": "function", "function": {"name": "get_weather", "description": "Get current conditions and a 3-day forecast for a location. Use this for ANY weather question instead of web_search or guessing. Safe read-only network operation.", "parameters": {"type": "object", "properties": {"location": {"type": "string", "description": "City name, optionally with region/country, e.g. \"Bhopal, India\". If the user does not specify a location, use their known location."}}, "required": ["location"]}}},
        {"type": "function", "function": {"name": "fetch_url", "description": "Fetch and read text content from a URL. Safe read-only network operation.", "parameters": {"type": "object", "properties": {"url": {"type": "string", "description": "URL beginning with http:// or https://."}}, "required": ["url"]}}},
        {"type": "function", "function": {"name": "get_current_directory", "description": "Get current working directory. Safe read-only operation.", "parameters": {"type": "object", "properties": {}}}},
        {"type": "function", "function": {"name": "get_direct_file_count", "description": "Count regular files directly inside current directory.", "parameters": {"type": "object", "properties": {}}}},
        {"type": "function", "function": {"name": "get_recursive_file_count", "description": "Count regular files recursively excluding .git.", "parameters": {"type": "object", "properties": {}}}},
        {"type": "function", "function": {"name": "list_directory", "description": "List files and directories. Safe read-only operation.", "parameters": {"type": "object", "properties": {"directory": {"type": "string", "description": "Directory path. Defaults to current directory."}}}}},
        {"type": "function", "function": {"name": "read_file", "description": "Read a text file. Safe read-only operation.", "parameters": {"type": "object", "properties": {"file": {"type": "string", "description": "File path."}}, "required": ["file"]}}},
        {"type": "function", "function": {"name": "find_files", "description": "Find files by filename. Safe read-only operation.", "parameters": {"type": "object", "properties": {"pattern": {"type": "string", "description": "Filename pattern."}}, "required": ["pattern"]}}},
        {"type": "function", "function": {"name": "search_files", "description": "Search text inside files. Safe read-only operation.", "parameters": {"type": "object", "properties": {"query": {"type": "string", "description": "Text or regex to search for."}}, "required": ["query"]}}},
        {"type": "function", "function": {"name": "get_git_status", "description": "Get Git status. Safe read-only operation.", "parameters": {"type": "object", "properties": {}}}},
        {"type": "function", "function": {"name": "get_git_diff", "description": "Read Git diff. Safe read-only operation.", "parameters": {"type": "object", "properties": {}}}},
        {"type": "function", "function": {"name": "run_command", "description": "Execute a shell command. ALWAYS requires user approval.", "parameters": {"type": "object", "properties": {"command": {"type": "string", "description": "Shell command to execute."}}, "required": ["command"]}}}
    ])
}

fn arg_str(args: &Value, key: &str, default: &str) -> String {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| default.to_string())
}

pub fn execute_tool(name: &str, arguments: &Value) -> String {
    match name {
        "get_current_time" => tool_current_time(),
        "web_search" => web::tool_web_search(&arg_str(arguments, "query", "")),
        "get_weather" => weather::tool_get_weather(&arg_str(arguments, "location", "")),
        "fetch_url" => web::tool_fetch_url(&arg_str(arguments, "url", "")),
        "get_current_directory" => get_current_directory(),
        "get_direct_file_count" => count_direct_files().to_string(),
        "get_recursive_file_count" => count_recursive_files().to_string(),
        "list_directory" => fs::tool_list_directory(&arg_str(arguments, "directory", ".")),
        "read_file" => fs::tool_read_file(&arg_str(arguments, "file", "")),
        "find_files" => fs::tool_find_files(&arg_str(arguments, "pattern", "")),
        "search_files" => fs::tool_search_files(&arg_str(arguments, "query", "")),
        "get_git_status" => git::tool_git_status(),
        "get_git_diff" => git::tool_git_diff(),
        "run_command" => shell::tool_run_command(&arg_str(arguments, "command", "")),
        other => format!("Unknown tool: {}", other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_schema_has_14_entries() {
        let tools = build_tools();
        assert_eq!(tools.as_array().map(|a| a.len()), Some(14));
    }
}
