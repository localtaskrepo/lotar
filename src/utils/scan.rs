/// Scan-related helpers shared by CLI and API surfaces.

#[derive(Default, Debug, Clone)]
pub struct InlineAttrs {
    pub assignee: Option<String>,
    pub priority: Option<String>,
    pub task_type: Option<String>,
    pub effort: Option<String>,
    pub due: Option<String>,
    pub tags: Vec<String>,
    pub fields: Vec<(String, String)>,
}

/// Strip inline attribute blocks like [key=value] without altering any other whitespace.
/// This preserves all spacing/alignment and only removes bracket sections that contain
/// an equals sign, which distinguishes them from language generics (e.g., Vec<String>)
/// or indexers (e.g., arr[0]).
pub fn strip_bracket_attributes(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut depth = 0usize;
    let mut buf = String::new(); // collect content when inside brackets

    for ch in line.chars() {
        match ch {
            '[' => {
                if depth == 0 {
                    // starting a new top-level bracket; reset buffer and decide later
                    buf.clear();
                } else {
                    // nested bracket content
                    buf.push('[');
                }
                depth += 1;
            }
            ']' => {
                if depth > 0 {
                    depth -= 1;
                    if depth == 0 {
                        // End of a top-level bracket. Decide whether to drop it.
                        // If the inner content contains an '=', treat it as an attribute and drop.
                        // Otherwise, keep the original bracket with its content.
                        if buf.contains('=') {
                            // drop entire [ ... ] including its content; do not write anything
                        } else {
                            out.push('[');
                            out.push_str(&buf);
                            out.push(']');
                        }
                        buf.clear();
                        continue;
                    } else {
                        // closing a nested level inside top-level; record literal
                        buf.push(']');
                        continue;
                    }
                }
                // Unbalanced ']' outside any bracket: write through
                out.push(']');
            }
            c => {
                if depth == 0 {
                    out.push(c);
                } else {
                    buf.push(c);
                }
            }
        }
    }
    // If brackets are unbalanced and we're still inside, write them back literally
    if depth > 0 {
        out.push('[');
        out.push_str(&buf);
    }
    out
}

/// Parse inline bracket attributes like [key=value] and map them to AddArgs fields.
/// Recognized keys (case-insensitive): assignee, priority, tags|tag, due|due_date,
/// type, effort. Unknown keys go into fields Vec.
pub fn parse_inline_attributes(line: &str) -> InlineAttrs {
    let mut attrs = Vec::new();
    // Collect top-level bracket contents
    let mut current = String::new();
    let mut depth = 0usize;
    for ch in line.chars() {
        match ch {
            '[' => {
                depth += 1;
                if depth == 1 {
                    current.clear();
                } else {
                    // nested, include the bracket content but we'll ignore for parsing simplicity
                    current.push('[');
                }
            }
            ']' => {
                if depth > 0 {
                    depth -= 1;
                    if depth == 0 {
                        if !current.trim().is_empty() {
                            attrs.push(current.trim().to_string());
                        }
                        current.clear();
                        continue;
                    } else {
                        current.push(']');
                    }
                }
            }
            c => {
                if depth > 0 {
                    current.push(c);
                }
            }
        }
    }

    let mut out = InlineAttrs::default();
    for a in attrs {
        // Allow comma-separated pairs within a single [ ... ]
        for part in a.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let (k, v) = if let Some((k, v)) = part.split_once('=') {
                (k.trim().to_lowercase(), v.trim().to_string())
            } else {
                // Single flag form [tag=foo] is preferred; if bare token present, skip
                continue;
            };
            match k.as_str() {
                "assignee" | "assign" => out.assignee = Some(v),
                "priority" => out.priority = Some(v),
                "type" => out.task_type = Some(v),
                "effort" => out.effort = Some(v),
                "due" | "due_date" => out.due = Some(v),
                "tag" => out.tags.push(v),
                "tags" => {
                    // Split on commas or whitespace
                    for t in v.split(|c: char| c == ',' || c.is_whitespace()) {
                        let t = t.trim();
                        if !t.is_empty() {
                            out.tags.push(t.to_string());
                        }
                    }
                }
                // ticket indicates an existing key; ignore here (handled elsewhere)
                "ticket" => {}
                _ => out.fields.push((k.to_string(), v)),
            }
        }
    }
    out
}

#[cfg(test)]
mod scan_utils_tests {
    use super::{parse_inline_attributes, strip_bracket_attributes};

    #[test]
    fn strip_preserves_leading_indentation() {
        let input = "\t    // TODO: Do it [assignee=me]  [priority=high]";
        let out = strip_bracket_attributes(input);
        assert!(out.starts_with("\t    // TODO: Do it"));
        // Ensure no brackets remain
        assert!(!out.contains('[') && !out.contains(']'));
        // Ensure indentation didn't collapse
        assert_eq!(&out[..5], "\t    ");
    }

    #[test]
    fn strip_does_not_collapse_spacing_or_alignments() {
        let input = "    signal_words: Vec<String>,                     // TODO handle words [tag=scan]  [due=2025-12-31]";
        let out = strip_bracket_attributes(input);
        // Leading spaces preserved
        assert!(out.starts_with(
            "    signal_words: Vec<String>,                     // TODO handle words"
        ));
        // No brackets remain
        assert!(!out.contains('[') && !out.contains(']'));
        // The run of spaces before the comment should still be long (>= 5)
        let after_comma = out.split("Vec<String>,").nth(1).unwrap_or("");
        // Expect at least 5 spaces before the // comment after the comma
        assert!(after_comma.starts_with("     "));
    }

    #[test]
    fn parse_inline_attributes_preserves_custom_field_key() {
        let attrs = parse_inline_attributes("// TODO tidy [product=Platform]");
        assert_eq!(
            attrs.fields,
            vec![("product".to_string(), "Platform".to_string())]
        );
    }
}
