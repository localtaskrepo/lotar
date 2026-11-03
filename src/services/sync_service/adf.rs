//! Jira's Atlassian Document Format (ADF) to and from Markdown-ish text
//! (DEV-262). Pulls used to glue words together and drop links, list
//! structure and line breaks; a later push then flattened the remote
//! description into one paragraph.
//!
//! Text form: blocks are separated by a blank line; a hard break is a
//! newline; `- ` bullet and `1. ` ordered list items; `#` headings; fenced
//! code blocks; `> ` quotes; `---` rules; links as `[text](url)` (or the bare
//! URL when text and URL are the same). Mentions, emoji, dates and status
//! lozenges become their text. Converting text to ADF and back returns the
//! same text, so unchanged descriptions are never pushed again.

use serde_json::{Map as JsonMap, Value as JsonValue, json};

pub(super) fn adf_to_text(value: &JsonValue) -> String {
    let Some(content) = value.get("content").and_then(|v| v.as_array()) else {
        return String::new();
    };
    blocks_to_text(content, "")
}

fn blocks_to_text(blocks: &[JsonValue], indent: &str) -> String {
    blocks
        .iter()
        .map(|block| block_to_text(block, indent))
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn node_type(node: &JsonValue) -> &str {
    node.get("type").and_then(|v| v.as_str()).unwrap_or("")
}

fn children(node: &JsonValue) -> &[JsonValue] {
    node.get("content")
        .and_then(|v| v.as_array())
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn block_to_text(node: &JsonValue, indent: &str) -> String {
    match node_type(node) {
        "paragraph" => inline_to_text(children(node)).trim().to_string(),
        "heading" => {
            let level = node
                .pointer("/attrs/level")
                .and_then(|v| v.as_u64())
                .unwrap_or(1)
                .clamp(1, 6) as usize;
            format!(
                "{} {}",
                "#".repeat(level),
                inline_to_text(children(node)).trim()
            )
        }
        "bulletList" | "orderedList" => list_to_text(node, indent),
        "codeBlock" => {
            let language = node
                .pointer("/attrs/language")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            format!(
                "```{language}\n{}\n```",
                inline_to_text(children(node)).trim_end_matches('\n')
            )
        }
        "blockquote" => blocks_to_text(children(node), "")
            .lines()
            .map(|line| {
                if line.is_empty() {
                    ">".to_string()
                } else {
                    format!("> {line}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        "rule" => "---".to_string(),
        "mediaSingle" | "mediaGroup" | "media" => String::new(),
        _ => {
            // Unknown blocks (panels, tables, ...): keep their text.
            if children(node).iter().any(is_inline) {
                inline_to_text(children(node)).trim().to_string()
            } else {
                blocks_to_text(children(node), indent)
            }
        }
    }
}

fn is_inline(node: &JsonValue) -> bool {
    matches!(
        node_type(node),
        "text" | "hardBreak" | "mention" | "emoji" | "date" | "inlineCard" | "status"
    )
}

fn list_to_text(node: &JsonValue, indent: &str) -> String {
    let ordered = node_type(node) == "orderedList";
    let start = node
        .pointer("/attrs/order")
        .and_then(|v| v.as_u64())
        .unwrap_or(1);
    let mut lines = Vec::new();
    for (index, item) in children(node).iter().enumerate() {
        let marker = if ordered {
            format!("{}. ", start + index as u64)
        } else {
            "- ".to_string()
        };
        let nested_indent = format!("{indent}  ");
        let mut first = true;
        for block in children(item) {
            if matches!(node_type(block), "bulletList" | "orderedList") {
                lines.push(list_to_text(block, &nested_indent));
                continue;
            }
            let text = block_to_text(block, &nested_indent);
            if text.is_empty() {
                continue;
            }
            for line in text.lines() {
                if first {
                    lines.push(format!("{indent}{marker}{line}"));
                    first = false;
                } else {
                    lines.push(format!("{nested_indent}{line}"));
                }
            }
        }
        if first {
            lines.push(format!("{indent}{}", marker.trim_end()));
        }
    }
    lines.join("\n")
}

fn inline_to_text(nodes: &[JsonValue]) -> String {
    let mut out = String::new();
    for node in nodes {
        let attr = |name: &str| {
            node.pointer(&format!("/attrs/{name}"))
                .and_then(|v| v.as_str())
                .map(str::to_string)
        };
        match node_type(node) {
            "text" => {
                let text = node.get("text").and_then(|v| v.as_str()).unwrap_or("");
                let href = node
                    .get("marks")
                    .and_then(|v| v.as_array())
                    .and_then(|marks| {
                        marks
                            .iter()
                            .find(|mark| node_type(mark) == "link")
                            .and_then(|mark| mark.pointer("/attrs/href"))
                            .and_then(|v| v.as_str())
                    });
                match href {
                    Some(href) if href == text => out.push_str(href),
                    Some(href) => out.push_str(&format!("[{text}]({href})")),
                    None => out.push_str(text),
                }
            }
            "hardBreak" => out.push('\n'),
            "mention" => out.push_str(&attr("text").unwrap_or_default()),
            "emoji" => out.push_str(
                &attr("text")
                    .or_else(|| attr("shortName"))
                    .unwrap_or_default(),
            ),
            "inlineCard" => out.push_str(&attr("url").unwrap_or_default()),
            "status" => out.push_str(&attr("text").unwrap_or_default()),
            "date" => {
                let rendered = attr("timestamp")
                    .and_then(|ms| ms.parse::<i64>().ok())
                    .and_then(chrono::DateTime::from_timestamp_millis)
                    .map(|date| date.format("%Y-%m-%d").to_string());
                out.push_str(&rendered.unwrap_or_default());
            }
            _ => {
                if let Some(text) = node.get("text").and_then(|v| v.as_str()) {
                    out.push_str(text);
                }
                out.push_str(&inline_to_text(children(node)));
            }
        }
    }
    out
}

pub(super) fn text_to_adf(text: &str) -> JsonValue {
    if text.trim().is_empty() {
        return JsonValue::Null;
    }
    let lines: Vec<&str> = text.lines().collect();
    json!({"type": "doc", "version": 1, "content": parse_blocks(&lines)})
}

fn parse_blocks(lines: &[&str]) -> Vec<JsonValue> {
    let mut blocks = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if line.trim().is_empty() {
            i += 1;
            continue;
        }
        if let Some(language) = line.strip_prefix("```") {
            let mut code = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].starts_with("```") {
                code.push(lines[i]);
                i += 1;
            }
            i += 1; // closing fence
            let mut node = json!({"type": "codeBlock", "content": []});
            if !language.trim().is_empty() {
                node["attrs"] = json!({"language": language.trim()});
            }
            if !code.is_empty() {
                node["content"] = json!([{"type": "text", "text": code.join("\n")}]);
            }
            blocks.push(node);
            continue;
        }
        // A block runs to the next blank line.
        let start = i;
        while i < lines.len() && !lines[i].trim().is_empty() && !lines[i].starts_with("```") {
            i += 1;
        }
        let chunk = &lines[start..i];
        blocks.push(parse_chunk(chunk));
    }
    blocks
}

fn list_marker(line: &str) -> Option<(usize, bool, Option<u64>, &str)> {
    let indent = line.len() - line.trim_start().len();
    let rest = &line[indent..];
    if let Some(item) = rest.strip_prefix("- ").or_else(|| rest.strip_prefix("* ")) {
        return Some((indent, false, None, item));
    }
    if rest == "-" {
        return Some((indent, false, None, ""));
    }
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    if digits > 0
        && let Some(item) = rest[digits..].strip_prefix(". ")
    {
        return Some((indent, true, rest[..digits].parse().ok(), item));
    }
    None
}

fn parse_chunk(chunk: &[&str]) -> JsonValue {
    if chunk.len() == 1
        && let Some(level) = heading_level(chunk[0])
    {
        let text = chunk[0][level..].trim();
        return json!({"type": "heading", "attrs": {"level": level}, "content": parse_inline(text)});
    }
    if chunk.iter().all(|line| line.trim() == "---") && chunk.len() == 1 {
        return json!({"type": "rule"});
    }
    if chunk.iter().all(|line| line.starts_with('>')) {
        let inner: Vec<&str> = chunk
            .iter()
            .map(|line| {
                line.strip_prefix("> ")
                    .or_else(|| line.strip_prefix('>'))
                    .unwrap_or("")
            })
            .collect();
        return json!({"type": "blockquote", "content": parse_blocks(&inner)});
    }
    if list_marker(chunk[0]).is_some_and(|(indent, ..)| indent == 0) {
        let (list, _) = parse_list(chunk, 0);
        return list;
    }
    paragraph(&chunk.join("\n"))
}

fn heading_level(line: &str) -> Option<usize> {
    let hashes = line.bytes().take_while(|b| *b == b'#').count();
    ((1..=6).contains(&hashes) && line[hashes..].starts_with(' ')).then_some(hashes)
}

fn paragraph(text: &str) -> JsonValue {
    json!({"type": "paragraph", "content": parse_inline(text)})
}

/// Parse list lines at `indent`; returns the list node and lines consumed.
fn parse_list(lines: &[&str], indent: usize) -> (JsonValue, usize) {
    let Some((_, ordered, order, _)) = list_marker(lines[0]) else {
        return (paragraph(lines[0]), 1);
    };
    let mut items: Vec<JsonValue> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let Some((item_indent, item_ordered, _, text)) = list_marker(lines[i]) else {
            // Continuation line of the previous item.
            if let Some(last) = items.last_mut()
                && let Some(content) = last["content"].as_array_mut()
            {
                content.push(paragraph(lines[i].trim()));
            }
            i += 1;
            continue;
        };
        if item_indent < indent || (item_indent == indent && item_ordered != ordered) {
            break;
        }
        if item_indent > indent {
            let (nested, used) = parse_list(&lines[i..], item_indent);
            if let Some(last) = items.last_mut()
                && let Some(content) = last["content"].as_array_mut()
            {
                content.push(nested);
            }
            i += used;
            continue;
        }
        let content = if text.is_empty() {
            vec![json!({"type": "paragraph", "content": []})]
        } else {
            vec![paragraph(text)]
        };
        items.push(json!({"type": "listItem", "content": content}));
        i += 1;
    }
    let mut node = JsonMap::new();
    node.insert(
        "type".into(),
        json!(if ordered { "orderedList" } else { "bulletList" }),
    );
    if ordered && order.unwrap_or(1) != 1 {
        node.insert("attrs".into(), json!({"order": order.unwrap_or(1)}));
    }
    node.insert("content".into(), JsonValue::Array(items));
    (JsonValue::Object(node), i.max(1))
}

/// Inline text: newlines become hard breaks; `[text](url)` and bare
/// http(s) URLs become link-marked text.
fn parse_inline(text: &str) -> Vec<JsonValue> {
    let mut nodes = Vec::new();
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            nodes.push(json!({"type": "hardBreak"}));
        }
        parse_links(line, &mut nodes);
    }
    nodes
}

fn push_text(nodes: &mut Vec<JsonValue>, text: &str, href: Option<&str>) {
    if text.is_empty() {
        return;
    }
    let mut node = json!({"type": "text", "text": text});
    if let Some(href) = href {
        node["marks"] = json!([{"type": "link", "attrs": {"href": href}}]);
    }
    nodes.push(node);
}

fn parse_links(line: &str, nodes: &mut Vec<JsonValue>) {
    let mut rest = line;
    let mut plain = String::new();
    while !rest.is_empty() {
        // [text](url)
        if rest.starts_with('[')
            && let Some(close) = rest.find("](")
            && let Some(end) = rest[close + 2..].find(')')
        {
            let label = &rest[1..close];
            let href = &rest[close + 2..close + 2 + end];
            if !label.contains('[') && is_url(href) {
                push_text(nodes, &std::mem::take(&mut plain), None);
                push_text(nodes, label, Some(href));
                rest = &rest[close + 2 + end + 1..];
                continue;
            }
        }
        if (rest.starts_with("http://") || rest.starts_with("https://"))
            && (plain.is_empty() || plain.ends_with(char::is_whitespace) || plain.ends_with('('))
        {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let url = rest[..end].trim_end_matches(['.', ',', ';', ':', ')', '!', '?']);
            if is_url(url) {
                push_text(nodes, &std::mem::take(&mut plain), None);
                push_text(nodes, url, Some(url));
                rest = &rest[url.len()..];
                continue;
            }
        }
        let ch = rest.chars().next().unwrap();
        plain.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    push_text(nodes, &plain, None);
}

fn is_url(value: &str) -> bool {
    (value.starts_with("http://") || value.starts_with("https://"))
        && value.len() > 8
        && !value.contains(char::is_whitespace)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(text: &str) {
        assert_eq!(
            adf_to_text(&text_to_adf(text)),
            text,
            "round trip of {text:?}"
        );
    }

    #[test]
    fn structure_survives_a_pull() {
        let adf = json!({"type": "doc", "version": 1, "content": [
            {"type": "paragraph", "content": [
                {"type": "text", "text": "Line one"},
                {"type": "hardBreak"},
                {"type": "text", "text": "line two"},
            ]},
            {"type": "bulletList", "content": [
                {"type": "listItem", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "alpha"}]}]},
                {"type": "listItem", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "beta"}]}]},
            ]},
            {"type": "paragraph", "content": [
                {"type": "text", "text": "see "},
                {"type": "inlineCard", "attrs": {"url": "https://example.com/spec"}},
                {"type": "text", "text": " and "},
                {"type": "text", "text": "docs", "marks": [{"type": "link", "attrs": {"href": "https://example.com/docs"}}]},
                {"type": "text", "text": ", ask "},
                {"type": "mention", "attrs": {"id": "1", "text": "@Jane"}},
                {"type": "text", "text": " "},
                {"type": "emoji", "attrs": {"shortName": ":tada:", "text": "🎉"}},
            ]},
        ]});
        assert_eq!(
            adf_to_text(&adf),
            "Line one\nline two\n\n- alpha\n- beta\n\nsee https://example.com/spec and [docs](https://example.com/docs), ask @Jane 🎉"
        );
    }

    #[test]
    fn text_round_trips_through_adf() {
        round_trip("Hello world");
        round_trip("Line one\nline two\n\n- alpha\n- beta\n  - nested\n\n1. first\n2. second");
        round_trip("# Title\n\nsee [docs](https://example.com/docs) or https://example.com.");
        round_trip("```rust\nfn main() {}\n```\n\n> quoted\n> more\n\n---");
        assert_eq!(text_to_adf("   "), JsonValue::Null);
    }

    #[test]
    fn pushed_text_keeps_lists_and_links() {
        let adf = text_to_adf("Intro\n\n- a\n- b\n\nsee https://x.example/y");
        let content = adf["content"].as_array().unwrap();
        assert_eq!(node_type(&content[0]), "paragraph");
        assert_eq!(node_type(&content[1]), "bulletList");
        assert_eq!(content[1]["content"].as_array().unwrap().len(), 2);
        let link = &content[2]["content"][1];
        assert_eq!(link["marks"][0]["attrs"]["href"], "https://x.example/y");
    }
}
