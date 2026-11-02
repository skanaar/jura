use serde_json::Value;

use super::{Comment, Issue, Transition};

pub(super) fn error_message(body: &str) -> String {
    if let Ok(v) = serde_json::from_str::<Value>(body) {
        let mut msgs: Vec<String> = v["errorMessages"]
            .as_array()
            .map(|a| a.iter().filter_map(|m| m.as_str().map(String::from)).collect())
            .unwrap_or_default();
        if let Some(errs) = v["errors"].as_object() {
            msgs.extend(errs.iter().map(|(k, m)| format!("{k}: {}", m.as_str().unwrap_or_default())));
        }
        if !msgs.is_empty() {
            return msgs.join("; ");
        }
    }
    let trimmed = body.trim();
    trimmed.chars().take(200).collect()
}

fn str_at(v: &Value, path: &[&str]) -> Option<String> {
    let mut cur = v;
    for p in path {
        cur = cur.get(p)?;
    }
    cur.as_str().map(String::from)
}

pub(super) fn parse_issue(v: &Value) -> Issue {
    let f = &v["fields"];
    Issue {
        key: str_at(v, &["key"]).unwrap_or_default(),
        summary: str_at(f, &["summary"]).unwrap_or_default(),
        status: str_at(f, &["status", "name"]).unwrap_or_else(|| "?".into()),
        status_category: str_at(f, &["status", "statusCategory", "key"]).unwrap_or_default(),
        issue_type: str_at(f, &["issuetype", "name"]).unwrap_or_default(),
        priority: str_at(f, &["priority", "name"]),
        assignee: str_at(f, &["assignee", "displayName"]),
        reporter: str_at(f, &["reporter", "displayName"]),
        created: str_at(f, &["created"]).unwrap_or_default(),
        updated: str_at(f, &["updated"]).unwrap_or_default(),
        labels: f["labels"]
            .as_array()
            .map(|a| a.iter().filter_map(|l| l.as_str().map(String::from)).collect())
            .unwrap_or_default(),
    }
}

pub(super) fn parse_comment(v: &Value) -> Comment {
    Comment {
        author: str_at(v, &["author", "displayName"]).unwrap_or_else(|| "?".into()),
        created: str_at(v, &["created"]).unwrap_or_default(),
        body: rich_text(&v["body"]),
        pending: false,
    }
}

pub(super) fn parse_transitions(v: &Value) -> Vec<Transition> {
    v["transitions"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|t| Transition {
                    id: str_at(t, &["id"]).unwrap_or_default(),
                    name: str_at(t, &["name"]).unwrap_or_default(),
                    to: str_at(t, &["to", "name"]).unwrap_or_default(),
                    to_category: str_at(t, &["to", "statusCategory", "key"]).unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Server returns wiki-markup strings, Cloud returns ADF documents.
pub(super) fn rich_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.replace("\r\n", "\n"),
        Value::Object(_) => adf_to_text(v),
        _ => String::new(),
    }
}

/// Flattens an Atlassian Document Format tree into readable plain text.
pub fn adf_to_text(doc: &Value) -> String {
    let mut out = String::new();
    adf_block(doc, &mut out);
    out.trim_end().to_string()
}

fn children(node: &Value) -> &[Value] {
    node["content"].as_array().map(Vec::as_slice).unwrap_or_default()
}

fn adf_block(node: &Value, out: &mut String) {
    match node["type"].as_str().unwrap_or_default() {
        "paragraph" | "heading" => {
            adf_inline(node, out);
            out.push_str("\n\n");
        }
        "bulletList" | "orderedList" => {
            let ordered = node["type"] == "orderedList";
            let start = node["attrs"]["order"].as_u64().unwrap_or(1);
            for (i, item) in children(node).iter().enumerate() {
                let prefix = if ordered { format!("{}. ", start + i as u64) } else { "• ".into() };
                let mut inner = String::new();
                for c in children(item) {
                    adf_block(c, &mut inner);
                }
                let pad = " ".repeat(prefix.chars().count());
                for (n, line) in inner.lines().filter(|l| !l.trim().is_empty()).enumerate() {
                    out.push_str(if n == 0 { &prefix } else { &pad });
                    out.push_str(line);
                    out.push('\n');
                }
            }
            out.push('\n');
        }
        "codeBlock" => {
            let mut code = String::new();
            adf_inline(node, &mut code);
            for line in code.lines() {
                out.push_str("    ");
                out.push_str(line);
                out.push('\n');
            }
            out.push('\n');
        }
        "blockquote" | "panel" => {
            let mut inner = String::new();
            for c in children(node) {
                adf_block(c, &mut inner);
            }
            for line in inner.trim_end().lines() {
                out.push_str("│ ");
                out.push_str(line);
                out.push('\n');
            }
            out.push('\n');
        }
        "rule" => out.push_str("────────\n\n"),
        "table" => {
            for row in children(node) {
                let cells: Vec<String> = children(row)
                    .iter()
                    .map(|cell| {
                        let mut s = String::new();
                        for c in children(cell) {
                            adf_block(c, &mut s);
                        }
                        s.split_whitespace().collect::<Vec<_>>().join(" ")
                    })
                    .collect();
                out.push_str(&cells.join(" │ "));
                out.push('\n');
            }
            out.push('\n');
        }
        "mediaSingle" | "mediaGroup" | "media" => out.push_str("[attachment]\n\n"),
        _ => {
            for c in children(node) {
                adf_block(c, out);
            }
            if node["text"].is_string() {
                adf_inline_node(node, out);
            }
        }
    }
}

fn adf_inline(node: &Value, out: &mut String) {
    for c in children(node) {
        adf_inline_node(c, out);
    }
}

fn adf_inline_node(node: &Value, out: &mut String) {
    let attrs = &node["attrs"];
    match node["type"].as_str().unwrap_or_default() {
        "text" => out.push_str(node["text"].as_str().unwrap_or_default()),
        "hardBreak" => out.push('\n'),
        "mention" => out.push_str(attrs["text"].as_str().unwrap_or("@someone")),
        "emoji" => out.push_str(attrs["text"].as_str().or(attrs["shortName"].as_str()).unwrap_or_default()),
        "inlineCard" | "blockCard" => out.push_str(attrs["url"].as_str().unwrap_or_default()),
        "status" => {
            out.push('[');
            out.push_str(attrs["text"].as_str().unwrap_or_default());
            out.push(']');
        }
        "date" => out.push_str(attrs["timestamp"].as_str().unwrap_or_default()),
        _ => adf_inline(node, out),
    }
}
