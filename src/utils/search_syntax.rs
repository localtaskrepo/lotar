//! The web search-box syntax, for `lotar list` (DEV-231).
//!
//! A port of `view/composables/useFilterGrammar.ts` (`parseFilterQuery`):
//! plain words become the search text, `key:value` tokens with a known key
//! become filters, `key=value` and `field:name=value` set a filter outright.
//! Unknown `key:value` tokens, URLs, `/regex/` terms and fully quoted
//! literals stay search text. A leading minus on a filter excludes it
//! (`-status:done`). Both parsers are checked against the shared
//! cases in `tests/fixtures/search_syntax_cases.json`, so keep them in step.

use std::collections::BTreeMap;

/// A parsed search: the search text, the filters by canonical key, and the
/// messages for malformed tokens (which are left out of the filters).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedSearch {
    pub text: String,
    pub filters: BTreeMap<String, String>,
    pub errors: Vec<String>,
}

/// A search key: its canonical name and the aliases accepted before `:`.
pub struct SearchKey {
    pub key: &'static str,
    pub aliases: &'static [&'static str],
    /// `mine:anything` means `mine=true`.
    pub flag: bool,
}

/// Keys usable as `key:value`, in the web UI's order.
pub const SEARCH_KEYS: &[SearchKey] = &[
    key("status", &["status", "state"]),
    key("priority", &["priority", "prio"]),
    key("type", &["type", "tasktype", "kind"]),
    key("sprints", &["sprint", "sprints"]),
    key("project", &["project", "projectkey"]),
    key("assignee", &["assignee", "owner"]),
    key("tags", &["tag", "tags"]),
    key("due", &["due", "duedate", "dueon"]),
    key("recent", &["recent"]),
    key("needs", &["needs", "need"]),
    key("has", &["has", "with"]),
    key("due_within", &["due_within", "duewithin"]),
    key("effort_min", &["effort_min", "effortmin"]),
    key("effort_max", &["effort_max", "effortmax"]),
    SearchKey {
        key: "mine",
        aliases: &["mine", "me"],
        flag: true,
    },
];

const fn key(key: &'static str, aliases: &'static [&'static str]) -> SearchKey {
    SearchKey {
        key,
        aliases,
        flag: false,
    }
}

/// Multi-select keys: repeated `key:value` tokens add up.
pub const LIST_KEYS: &[&str] = &[
    "status", "priority", "type", "sprints", "tags", "needs", "has",
];

/// Routing keys a leading minus cannot exclude.
const UNNEGATABLE_KEYS: &[&str] = &["q", "order", "sort_by", "deletion", "archived"];

/// The search key for an alias, ignoring case.
pub fn find_key(token: &str) -> Option<&'static SearchKey> {
    let lower = token.to_lowercase();
    SEARCH_KEYS
        .iter()
        .find(|meta| meta.aliases.contains(&lower.as_str()))
}

/// Builtin target of a `field:<name>` reserved name, if any.
fn reserved_field_alias(name: &str) -> Option<&'static str> {
    let normalized: String = name
        .to_lowercase()
        .chars()
        .filter(|ch| !matches!(ch, '-' | '_') && !ch.is_whitespace())
        .collect();
    match normalized.as_str() {
        "q" | "query" | "text" | "textquery" | "search" => return Some("q"),
        "order" | "sort" => return Some("order"),
        "sortby" => return Some("sort_by"),
        "deletion" => return Some("deletion"),
        "archived" => return Some("archived"),
        _ => {}
    }
    SEARCH_KEYS.iter().find_map(|meta| {
        let matches = |alias: &str| alias.replace('_', "") == normalized;
        (matches(meta.key) || meta.aliases.iter().any(|alias| matches(alias))).then_some(meta.key)
    })
}

struct Segment {
    value: String,
    quoted: bool,
    ok: bool,
}

/// A cleanly quoted segment unescapes to its content; anything else is
/// returned as is. Backslash escapes only apply inside quotes.
fn unquote(raw: &str) -> Segment {
    let chars: Vec<char> = raw.chars().collect();
    let Some(&quote) = chars.first().filter(|c| matches!(c, '"' | '\'')) else {
        return Segment {
            value: raw.to_string(),
            quoted: false,
            ok: true,
        };
    };
    let not_ok = || Segment {
        value: raw.to_string(),
        quoted: false,
        ok: false,
    };
    let mut value = String::new();
    let mut i = 1;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '\\' && i + 1 < chars.len() && (chars[i + 1] == quote || chars[i + 1] == '\\') {
            value.push(chars[i + 1]);
            i += 2;
            continue;
        }
        if ch == quote {
            return if i == chars.len() - 1 {
                Segment {
                    value,
                    quoted: true,
                    ok: true,
                }
            } else {
                not_ok()
            };
        }
        value.push(ch);
        i += 1;
    }
    not_ok()
}

/// Split on separator characters outside quotes, keeping the raw text.
fn split_top_level(raw: &str, is_sep: impl Fn(char) -> bool) -> Vec<String> {
    let chars: Vec<char> = raw.chars().collect();
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if let Some(q) = quote {
            if ch == '\\'
                && let Some(&next) = chars.get(i + 1)
                && (next == q || next == '\\')
            {
                current.push(ch);
                current.push(next);
                i += 2;
                continue;
            }
            if ch == q {
                quote = None;
            }
            current.push(ch);
        } else if ch == '"' || ch == '\'' {
            quote = Some(ch);
            current.push(ch);
        } else if is_sep(ch) {
            parts.push(std::mem::take(&mut current));
        } else {
            current.push(ch);
        }
        i += 1;
    }
    parts.push(current);
    parts
}

/// Character index of the first `target` outside quotes.
fn find_top_level(raw: &str, target: char) -> Option<usize> {
    let chars: Vec<char> = raw.chars().collect();
    let mut quote: Option<char> = None;
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if let Some(q) = quote {
            if ch == '\\' && matches!(chars.get(i + 1), Some(&next) if next == q || next == '\\') {
                i += 2;
                continue;
            }
            if ch == q {
                quote = None;
            }
        } else if ch == '"' || ch == '\'' {
            quote = Some(ch);
        } else if ch == target {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn before(raw: &str, index: usize) -> String {
    raw.chars().take(index).collect()
}

fn after(raw: &str, index: usize) -> String {
    raw.chars().skip(index + 1).collect()
}

/// `scheme://` tokens (URLs) never become filters or get split apart.
fn is_url_like(raw: &str) -> bool {
    let mut chars = raw.chars();
    if !chars.next().is_some_and(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    let rest: String = chars.collect();
    let scheme_end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-')))
        .unwrap_or(rest.len());
    rest[scheme_end..].starts_with("://")
}

fn is_field_key(key: &str) -> bool {
    key.to_lowercase().starts_with("field:")
}

/// Whether a comma segment has the shape of an assignment or a known
/// `key:value` token.
fn looks_like_expression(segment: &str) -> bool {
    if is_url_like(segment) {
        return false;
    }
    if let Some(eq) = find_top_level(segment, '=') {
        let key_raw = before(segment, eq);
        if find_top_level(&key_raw, ':').is_none() {
            return true;
        }
        return is_field_key(&unquote(&key_raw).value);
    }
    match find_top_level(segment, ':') {
        Some(colon) if colon > 0 => find_key(&unquote(&before(segment, colon)).value).is_some(),
        _ => false,
    }
}

/// Split `a=1,b=2` into tokens, but only when every comma segment is an
/// expression, so values such as `tags:ci,ops` stay whole.
fn split_comma_expressions(token: &str) -> Vec<String> {
    let parts = split_top_level(token, |ch| ch == ',');
    if parts.len() < 2 {
        return vec![token.to_string()];
    }
    let non_empty: Vec<String> = parts.into_iter().filter(|p| !p.is_empty()).collect();
    if non_empty.is_empty() || !non_empty.iter().all(|p| looks_like_expression(p)) {
        return vec![token.to_string()];
    }
    non_empty
}

struct Parser {
    parsed: ParsedSearch,
    words: Vec<String>,
    /// Whether the last token for a key was an `=` assignment.
    assigned: BTreeMap<String, bool>,
}

impl Parser {
    fn assignment(&mut self, raw: &str, eq: usize) {
        let key_seg = unquote(&before(raw, eq));
        let value_seg = unquote(&after(raw, eq));
        if !key_seg.ok || !value_seg.ok {
            self.parsed
                .errors
                .push(format!("Unmatched quote in \"{raw}\""));
            return;
        }
        let key_text = if key_seg.quoted {
            key_seg.value
        } else {
            key_seg.value.trim().to_string()
        };
        let value = if value_seg.quoted {
            value_seg.value
        } else {
            value_seg.value.trim().to_string()
        };
        let display = {
            let head = before(raw, eq);
            let head = head.trim();
            if head.is_empty() {
                raw.to_string()
            } else {
                head.to_string()
            }
        };

        if is_field_key(&key_text) {
            let name_raw = key_text[key_text.find(':').unwrap_or(0) + 1..].trim();
            let name_seg = unquote(name_raw);
            if !name_seg.ok {
                self.parsed
                    .errors
                    .push(format!("Unmatched quote in \"{raw}\""));
                return;
            }
            let name = if name_seg.quoted {
                name_seg.value.clone()
            } else {
                name_seg.value.trim().to_string()
            };
            if name.is_empty() {
                self.parsed
                    .errors
                    .push(format!("Invalid filter name \"{display}\""));
                return;
            }
            if value.is_empty() {
                self.parsed
                    .errors
                    .push(format!("Add a value for \"{display}\""));
                return;
            }
            let builtin = if name_seg.quoted {
                None
            } else {
                reserved_field_alias(&name)
            };
            let key = builtin
                .map(str::to_string)
                .unwrap_or_else(|| format!("field:{}", name.to_lowercase()));
            self.set(key, value, true);
            return;
        }

        if key_text.is_empty() {
            self.parsed
                .errors
                .push("Missing key before \"=\"".to_string());
            return;
        }
        if value.is_empty() {
            self.parsed
                .errors
                .push(format!("Add a value for \"{key_text}\""));
            return;
        }
        // Bare equality never maps aliases: `owner=ops` stays a custom
        // `owner` filter, unlike `owner:ops`.
        self.set(key_text.to_lowercase(), value, true);
    }

    fn colon(&mut self, raw: &str, colon: usize) {
        let key_seg = unquote(&before(raw, colon));
        if !key_seg.ok {
            self.parsed
                .errors
                .push(format!("Unmatched quote in \"{raw}\""));
            return;
        }
        let Some(meta) = find_key(key_seg.value.trim()) else {
            self.words.push(raw.to_string());
            return;
        };
        let value_seg = unquote(&after(raw, colon));
        if !value_seg.ok {
            self.parsed
                .errors
                .push(format!("Unmatched quote in \"{raw}\""));
            return;
        }
        let value = if value_seg.quoted {
            value_seg.value
        } else {
            value_seg.value.trim().to_string()
        };
        if value.is_empty() {
            self.words.push(raw.to_string());
            return;
        }
        let next = if meta.flag { "true".to_string() } else { value };
        let adds = self.assigned.get(meta.key) == Some(&false) && LIST_KEYS.contains(&meta.key);
        let value = match self.parsed.filters.get(meta.key) {
            Some(previous) if adds => format!("{previous},{next}"),
            _ => next,
        };
        self.set(meta.key.to_string(), value, false);
    }

    /// Parse `raw` (starting with `-`) as an excluded filter; false when the
    /// rest is not a single filter, so the token stays search text.
    fn negation(&mut self, raw: &str) -> bool {
        let mut inner = Parser {
            parsed: ParsedSearch::default(),
            words: Vec::new(),
            assigned: BTreeMap::new(),
        };
        inner.token(&raw[1..]);
        if !inner.parsed.errors.is_empty() {
            self.parsed.errors.append(&mut inner.parsed.errors);
            return true;
        }
        if inner.parsed.filters.len() != 1 || !inner.words.is_empty() {
            return false;
        }
        let (key, value) = inner.parsed.filters.into_iter().next().expect("one filter");
        if UNNEGATABLE_KEYS.contains(&key.as_str()) {
            self.parsed
                .errors
                .push(format!("\"{raw}\" cannot be excluded"));
            return true;
        }
        let negated = format!("-{key}");
        let value = match self.parsed.filters.get(&negated) {
            Some(previous) => format!("{previous},{value}"),
            None => value,
        };
        self.parsed.filters.insert(negated, value);
        true
    }

    fn set(&mut self, key: String, value: String, assigned: bool) {
        self.assigned.insert(key.clone(), assigned);
        self.parsed.filters.insert(key, value);
    }

    fn token(&mut self, raw: &str) {
        // A fully quoted literal is search text; it keeps its double quotes
        // so it is searched as one phrase.
        let whole = unquote(raw);
        if whole.quoted {
            self.words.push(if whole.value.contains('"') {
                whole.value
            } else {
                format!("\"{}\"", whole.value)
            });
            return;
        }
        // URLs and `/regex/` terms are search text even with `=` or `:`.
        if is_url_like(raw) || raw.starts_with('/') || raw.starts_with("-/") {
            self.words.push(raw.to_string());
            return;
        }
        // A leading minus on a filter excludes it (DEV-5): `-status:done`,
        // `-field:area=billing`. Negations of one key add up. A minus on
        // anything else (`-oauth`, `-title:draft`) stays search text.
        if raw.len() > 1 && raw.starts_with('-') && !raw.starts_with("--") && self.negation(raw) {
            return;
        }
        let eq = find_top_level(raw, '=');
        let colon = find_top_level(raw, ':');
        if let Some(eq) = eq
            && (colon.is_none_or(|c| c > eq) || is_field_key(&unquote(&before(raw, eq)).value))
        {
            self.assignment(raw, eq);
            return;
        }
        if let Some(colon) = colon {
            self.colon(raw, colon);
            return;
        }
        self.words.push(raw.to_string());
    }
}

/// Parse a search-box query.
pub fn parse(input: &str) -> ParsedSearch {
    let mut parser = Parser {
        parsed: ParsedSearch::default(),
        words: Vec::new(),
        assigned: BTreeMap::new(),
    };
    for raw in split_top_level(input, char::is_whitespace)
        .into_iter()
        .filter(|t| !t.is_empty())
    {
        for token in split_comma_expressions(&raw) {
            parser.token(&token);
        }
    }
    parser.parsed.text = parser.words.join(" ");
    parser.parsed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize)]
    struct Case {
        input: String,
        text: String,
        filters: BTreeMap<String, String>,
        #[serde(default)]
        errors: Vec<String>,
    }

    /// The cases the web UI parser is checked against too.
    #[test]
    fn matches_the_shared_cases() {
        let cases: Vec<Case> = serde_json::from_str(include_str!(
            "../../tests/fixtures/search_syntax_cases.json"
        ))
        .unwrap();
        assert!(cases.len() > 20);
        for case in cases {
            let parsed = parse(&case.input);
            assert_eq!(parsed.text, case.text, "text of {:?}", case.input);
            assert_eq!(parsed.filters, case.filters, "filters of {:?}", case.input);
            assert_eq!(parsed.errors, case.errors, "errors of {:?}", case.input);
        }
    }
}
