//! `fetch_url` and `web_search` (DuckDuckGo HTML scraping, with a
//! lite-endpoint fallback if the primary markup changes shape).

use regex::Regex;
use std::time::Duration;

use crate::consts::{VERSION, WEB_SEARCH_UA};
use crate::http::http_client;

fn strip_html(input: &str) -> String {
    let re_script = Regex::new(r"(?is)<script[^>]*>.*?</script>").expect("script regex");
    let re_style = Regex::new(r"(?is)<style[^>]*>.*?</style>").expect("style regex");
    let re_tag = Regex::new(r"<[^>]*>").expect("tag regex");
    let mut s = re_script.replace_all(input, "").to_string();
    s = re_style.replace_all(&s, "").to_string();
    s = re_tag.replace_all(&s, " ").to_string();
    s = s
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    s.lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn tool_fetch_url(url: &str) -> String {
    if url.is_empty() {
        return String::from("No URL supplied.");
    }
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return String::from("Only http:// and https:// URLs are supported.");
    }
    let client = http_client();
    let resp = match client
        .get(url)
        .header("User-Agent", format!("Pocai/{}", VERSION))
        .send()
    {
        Ok(r) => r,
        Err(_) => return format!("Failed to fetch URL: {}", url),
    };
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let body = match resp.text() {
        Ok(t) => t,
        Err(_) => return format!("Failed to fetch URL: {}", url),
    };
    let mut out = format!("URL: {}\n", url);
    if !content_type.is_empty() {
        out.push_str(&format!("content-type: {}\n", content_type));
    }
    out.push('\n');
    let lower = body.to_lowercase();
    if lower.contains("<html") || lower.contains("<!doctype html") {
        let stripped: String = strip_html(&body).chars().take(80_000).collect();
        out.push_str(&stripped);
    } else {
        out.push_str(&body.chars().take(80_000).collect::<String>());
    }
    out
}

fn clean_html_fragment(s: &str) -> String {
    let re_tag = Regex::new(r"<[^>]+>").expect("tag regex");
    let t = re_tag.replace_all(s, "");
    t.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn ddg_try(query: &str, endpoint: &str, link_pat: &str, snippet_pat: &str) -> Option<String> {
    let client = http_client();
    let body = client
        .get(endpoint)
        .header("User-Agent", WEB_SEARCH_UA)
        .header("Accept-Language", "en-US,en;q=0.9")
        .query(&[("q", query)])
        .timeout(Duration::from_secs(20))
        .send()
        .ok()?;
    if !body.status().is_success() {
        return None;
    }
    let html = body.text().ok()?;
    let link_re = Regex::new(&format!("(?i){}", link_pat)).ok()?;
    let snip_re = Regex::new(&format!("(?i){}", snippet_pat)).ok()?;

    let mut out = String::new();
    let mut title = String::new();
    let mut count = 0usize;
    for line in html.lines() {
        if link_re.is_match(line) {
            let t = clean_html_fragment(line);
            if !t.is_empty() {
                title = t;
            }
            continue;
        }
        if snip_re.is_match(line) {
            let snippet = clean_html_fragment(line);
            if !title.is_empty() {
                count += 1;
                out.push_str(&format!("{}. {}\n", count, title));
                if !snippet.is_empty() {
                    out.push_str(&format!("   {}\n", snippet));
                }
                out.push('\n');
                title.clear();
            }
        }
    }
    if count == 0 {
        return None;
    }
    Some(out.lines().take(80).collect::<Vec<_>>().join("\n"))
}

pub fn tool_web_search(query: &str) -> String {
    if query.is_empty() {
        return String::from("No search query supplied.");
    }
    let mut out = format!("Search query: {}\n\n", query);
    if let Some(results) = ddg_try(
        query,
        "https://html.duckduckgo.com/html/",
        r#"class="result__a""#,
        r#"class="result__snippet""#,
    ) {
        out.push_str(&results);
        out.push('\n');
        return out;
    }
    if let Some(results) = ddg_try(
        query,
        "https://lite.duckduckgo.com/lite/",
        r#"class="result-link""#,
        "result-snippet",
    ) {
        out.push_str(&results);
        out.push('\n');
        return out;
    }
    out.push_str(
        "Web search failed: both search endpoints were unreachable or blocked the request.\n\
         Try fetch_url with a specific URL instead, or tell the user the search failed.\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_html_basic() {
        let out = strip_html("<html><body><h1>Hi</h1><p>a &amp; b</p></body></html>");
        assert!(out.contains("Hi"));
        assert!(out.contains("a & b"));
        assert!(!out.contains('<'));
    }
}
