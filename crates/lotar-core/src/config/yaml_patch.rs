//! Rewrite only the changed keys of a hand-written config file (DEV-407,
//! DEV-811): comments, key order, spelling and the formatting of every other
//! key survive LoTaR's own writes.

use serde_yaml_ng::{Mapping, Value};

/// `existing` with the values that differ between the canonical renderings
/// `old` (of `existing`) and `new` patched in. `None` when a change can't be
/// patched in, a field ends up spelled twice, or `verify` rejects the text:
/// the caller then writes canonical YAML.
pub(crate) fn patch(existing: &str, old: &str, new: &str, verify: impl Fn(&str) -> bool) -> Option<String> {
    let new_root = canonical_mapping(new)?;
    let mut changes = Vec::new();
    diff(&canonical_mapping(old)?, &new_root, &mut Vec::new(), &mut changes)?;
    let mut lines: Vec<String> = existing.lines().map(str::to_string).collect();
    for path in &changes {
        apply(&mut lines, path, &new_root);
    }
    let mut text = lines.join("\n");
    if !text.is_empty() && (existing.is_empty() || existing.ends_with('\n')) {
        text.push('\n');
    }
    let raw = crate::config::normalization::expand_dotted_keys(serde_yaml_ng::from_str(&text).ok()?).ok()?;
    if changes.iter().any(|path| spelled_twice(&raw, path)) {
        return None;
    }
    verify(&text).then_some(text)
}

fn canonical_mapping(yaml: &str) -> Option<Mapping> {
    match serde_yaml_ng::from_str(yaml).ok()? {
        Value::Mapping(map) => Some(map),
        Value::Null => Some(Mapping::new()),
        _ => None,
    }
}

/// Paths of the values that differ, descending into mappings both sides have.
fn diff(old: &Mapping, new: &Mapping, path: &mut Vec<String>, out: &mut Vec<Vec<String>>) -> Option<()> {
    for key in old.keys().chain(new.keys().filter(|key| !old.contains_key(*key))) {
        let (before, after) = (old.get(key), new.get(key));
        if before == after {
            continue;
        }
        path.push(key.as_str()?.to_string());
        match (before, after) {
            (Some(Value::Mapping(before)), Some(Value::Mapping(after))) => diff(before, after, path, out)?,
            _ => out.push(path.clone()),
        }
        path.pop();
    }
    Some(())
}

/// Whether a config field the change touches is set under two spellings
/// (say flat `tags` and nested `issue.tags`), which the patch must not leave.
fn spelled_twice(raw: &Value, path: &[String]) -> bool {
    (1..=path.len()).any(|len| {
        crate::config::fields::flat_for_alias(&path[..len].join(".")).is_some_and(|flat| {
            let aliases = crate::config::fields::alias_paths().filter(|(alias, field)| *field == flat && *alias != flat).map(|(alias, _)| alias);
            std::iter::once(flat).chain(aliases).filter(|spelling| spelling.split('.').try_fold(raw, |value, part| value.get(part)).is_some()).count() > 1
        })
    })
}

fn lookup<'a>(root: &'a Mapping, path: &[String]) -> Option<&'a Value> {
    let (first, rest) = path.split_first()?;
    rest.iter().try_fold(root.get(first.as_str())?, |value, part| value.get(part.as_str()))
}

/// One `key: value` entry of the file: its lines `line..end`, the key as
/// spelled, the inline value and the comment after it.
struct Entry {
    line: usize,
    end: usize,
    key: String,
    value: String,
    comment: Option<String>,
}

/// Write the new value at `path` (or remove it) into the file's lines. The
/// path is looked up level by level, also in dotted (`issue.types:`) and,
/// at the top, flat (`issue_types:`) spellings; a missing key is appended
/// to the end of its parent block.
fn apply(lines: &mut Vec<String>, path: &[String], root: &Mapping) {
    let (mut start, mut end, mut indent, mut depth) = (0, lines.len(), 0, 0);
    while depth < path.len() {
        let found = (depth..path.len()).find_map(|last| {
            let dotted = path[depth..=last].join(".");
            let flat = if depth == 0 { crate::config::fields::flat_for_alias(&dotted) } else { None };
            find_entry(lines, start, end, indent, &dotted)
                .or_else(|| flat.and_then(|flat| find_entry(lines, start, end, indent, flat)))
                .map(|entry| (last, entry))
        });
        let Some((last, entry)) = found else {
            if let Some(value) = lookup(root, &path[..=depth]) {
                let at = without_trailing_blanks(lines, start, end);
                lines.splice(at..at, render(&path[depth], value, indent));
                return;
            }
            // A removed section may be spelled as dotted keys (`default.assignee:`).
            let prefix = format!("{}.", path[depth]);
            while let Some(entry) = (start..end).find_map(|index| {
                let line = &lines[index];
                let key = line.get(indent..).filter(|_| leading_spaces(line) == indent && line[indent..].starts_with(&prefix))?.split(':').next()?;
                find_entry(lines, index, end, indent, key)
            }) {
                end -= entry.end - entry.line;
                lines.drain(entry.line..entry.end);
            }
            return;
        };
        let Some(value) = lookup(root, &path[..=last]) else {
            lines.drain(entry.line..entry.end);
            return;
        };
        if last + 1 < path.len()
            && value.is_mapping()
            && let Some(child) = child_indent(lines, &entry, indent)
        {
            (start, end, indent, depth) = (entry.line + 1, entry.end, child, last + 1);
            continue;
        }
        let replacement = replace(lines, &entry, indent, value);
        lines.splice(entry.line..entry.end, replacement);
        return;
    }
}

fn leading_spaces(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

fn is_content(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty() && !trimmed.starts_with('#')
}

fn without_trailing_blanks(lines: &[String], start: usize, mut end: usize) -> usize {
    while end > start && lines[end - 1].trim().is_empty() {
        end -= 1;
    }
    end
}

fn find_entry(lines: &[String], start: usize, end: usize, indent: usize, key: &str) -> Option<Entry> {
    let rest_of = |line: &str| -> Option<String> {
        let rest = line.get(indent..)?.strip_prefix(key)?.strip_prefix(':')?;
        rest.chars().next().is_none_or(char::is_whitespace).then(|| rest.to_string())
    };
    let (line, rest) =
        (start..end).find_map(|index| (leading_spaces(&lines[index]) == indent).then(|| rest_of(&lines[index])).flatten().map(|rest| (index, rest)))?;
    // The entry runs over comment and blank lines at any indent, deeper
    // lines, and `- item` lines at its own indent; the comments after its
    // last content line belong to the next key.
    let mut stop = line + 1;
    while stop < end {
        let next = &lines[stop];
        let spaces = leading_spaces(next);
        if !is_content(next) || spaces > indent || (spaces == indent && next[indent..].starts_with('-')) {
            stop += 1;
        } else {
            break;
        }
    }
    while stop > line + 1 && !is_content(&lines[stop - 1]) {
        stop -= 1;
    }
    let (value, comment) = split_comment(&rest);
    Some(Entry { line, end: stop, key: key.to_string(), value, comment })
}

/// The indent of a block mapping's keys, `None` when the entry's value is
/// written inline or as a sequence.
fn child_indent(lines: &[String], entry: &Entry, indent: usize) -> Option<usize> {
    if !entry.value.is_empty() {
        return None;
    }
    match lines[entry.line + 1..entry.end].iter().find(|line| is_content(line)) {
        None => Some(indent + 2),
        Some(line) if line.trim_start().starts_with('-') => None,
        Some(line) => Some(leading_spaces(line)),
    }
}

/// The entry's new lines: a block keeps its key line (and comment) and item
/// indent, a flow list stays a flow list, a scalar keeps its comment.
fn replace(lines: &[String], entry: &Entry, indent: usize, value: &Value) -> Vec<String> {
    let block_value = match value {
        Value::Sequence(items) => !items.is_empty(),
        Value::Mapping(map) => !map.is_empty(),
        _ => false,
    };
    if entry.value.is_empty() && block_value {
        if let Some(kept) = edit_scalar_items(lines, entry, value) {
            return kept;
        }
        let first = lines[entry.line + 1..entry.end].iter().find(|line| is_content(line));
        let body = match (value, first) {
            (Value::Mapping(_), Some(line)) if !line.trim_start().starts_with('-') => leading_spaces(line),
            (Value::Mapping(_), _) => indent + 2,
            (_, Some(line)) => leading_spaces(line),
            (_, None) => indent,
        };
        let pad = " ".repeat(body);
        return std::iter::once(lines[entry.line].clone()).chain(yaml_lines(value).into_iter().map(|line| format!("{pad}{line}"))).collect();
    }
    let comment = entry.comment.as_ref().map(|comment| format!(" {comment}")).unwrap_or_default();
    if entry.value.starts_with('[')
        && let Some(flow) = flow_sequence(value)
    {
        return vec![format!("{}{}: {flow}{comment}", " ".repeat(indent), entry.key)];
    }
    let mut out = render(&entry.key, value, indent);
    if let Some(first) = out.first_mut() {
        first.push_str(&comment);
    }
    out
}

/// A block list of one-line scalar items that only loses items and gains
/// new ones at its end: its remaining lines stay as written, with the
/// comments between them.
fn edit_scalar_items(lines: &[String], entry: &Entry, value: &Value) -> Option<Vec<String>> {
    let Value::Sequence(new_items) = value else {
        return None;
    };
    let body = &lines[entry.line + 1..entry.end];
    let mut items = Vec::new();
    for (index, line) in body.iter().enumerate().filter(|(_, line)| is_content(line)) {
        let item = line.trim_start().strip_prefix('-').filter(|rest| rest.is_empty() || rest.starts_with(' '))?;
        let item: Value = serde_yaml_ng::from_str(&split_comment(item).0).ok()?;
        if item.is_mapping() || item.is_sequence() {
            return None;
        }
        items.push((index, leading_spaces(line), item));
    }
    let kept: Vec<&Value> = items.iter().map(|(_, _, item)| item).filter(|item| new_items.contains(item)).collect();
    if kept.is_empty() || new_items.len() < kept.len() || !new_items.iter().zip(&kept).all(|(new, old)| new == *old) {
        return None;
    }
    let removed: Vec<usize> = items.iter().filter(|(_, _, item)| !new_items.contains(item)).map(|(index, _, _)| *index).collect();
    let pad = " ".repeat(items.first()?.1);
    let added = new_items[kept.len()..].iter().flat_map(|item| yaml_lines(&Value::Sequence(vec![item.clone()]))).map(|line| format!("{pad}{line}"));
    let mut out: Vec<String> = std::iter::once(lines[entry.line].clone())
        .chain(body.iter().enumerate().filter(|(index, _)| !removed.contains(index)).map(|(_, line)| line.clone()))
        .collect();
    let at = out.iter().rposition(|line| is_content(line)).map_or(out.len(), |index| index + 1);
    out.splice(at..at, added);
    Some(out)
}

fn yaml_lines(value: &Value) -> Vec<String> {
    serde_yaml_ng::to_string(value).unwrap_or_default().lines().map(str::to_string).collect()
}

fn render(key: &str, value: &Value, indent: usize) -> Vec<String> {
    let mut map = Mapping::new();
    map.insert(Value::String(key.to_string()), value.clone());
    let pad = " ".repeat(indent);
    yaml_lines(&Value::Mapping(map)).into_iter().map(|line| format!("{pad}{line}")).collect()
}

/// `[a, b, c]` for a list of scalars that read the same in flow style.
fn flow_sequence(value: &Value) -> Option<String> {
    let Value::Sequence(items) = value else {
        return None;
    };
    let items = items
        .iter()
        .map(|item| {
            if item.is_mapping() || item.is_sequence() {
                return None;
            }
            let text = serde_yaml_ng::to_string(item).ok()?.trim_end().to_string();
            (!text.contains(['\n', ',', '[', ']', '{', '}', '#'])).then_some(text)
        })
        .collect::<Option<Vec<_>>>()?;
    Some(format!("[{}]", items.join(", ")))
}

/// Split the text after `key:` into its value and a trailing `# comment`.
fn split_comment(rest: &str) -> (String, Option<String>) {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut previous = ' ';
    for (index, c) in rest.char_indices() {
        match quote {
            Some('"') if escaped => escaped = false,
            Some('"') if c == '\\' => escaped = true,
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '#' && previous.is_whitespace() => return (rest[..index].trim().to_string(), Some(rest[index..].trim_end().to_string())),
            None if matches!(c, '\'' | '"') && (previous.is_whitespace() || matches!(previous, '[' | ',' | '{')) => quote = Some(c),
            None => {}
        }
        previous = c;
    }
    (rest.trim().to_string(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patched(existing: &str, new: &str) -> Option<String> {
        let raw = crate::config::normalization::expand_dotted_keys(serde_yaml_ng::from_str(existing).unwrap()).unwrap();
        let old = serde_yaml_ng::to_string(&raw).unwrap();
        patch(existing, &old, new, |_| true)
    }

    #[test]
    fn a_changed_key_keeps_comments_and_other_keys() {
        let existing = "issue:\n  types:\n  - Bug\nagent:\n  worktree:\n    # why 3\n    max_parallel_jobs: 3 # owner\n    waiting_run: [cargo, clean]\n";
        let new = "issue:\n  types: [Bug, Chore]\nagent:\n  worktree:\n    max_parallel_jobs: 4\n    waiting_run: [cargo, clean]\n";
        assert_eq!(
            patched(existing, new).unwrap(),
            "issue:\n  types:\n  - Bug\n  - Chore\nagent:\n  worktree:\n    # why 3\n    max_parallel_jobs: 4 # owner\n    waiting_run: [cargo, clean]\n"
        );
    }

    #[test]
    fn a_flow_list_stays_a_flow_list_and_new_keys_join_their_block() {
        let existing = "# top\nagent:\n  worktree:\n    waiting_run: [cargo, clean] # why\n";
        let new = "agent:\n  worktree:\n    waiting_run: [cargo, clean, --workspace]\n    max_parallel_jobs: 2\ndefault:\n  assignee: alice\n";
        assert_eq!(
            patched(existing, new).unwrap(),
            "# top\nagent:\n  worktree:\n    waiting_run: [cargo, clean, --workspace] # why\n    max_parallel_jobs: 2\ndefault:\n  assignee: alice\n"
        );
    }

    #[test]
    fn dotted_keys_are_patched_and_removed_keys_dropped() {
        let existing = "# a\nissue.types: [Bug]\ndefault.assignee: bob # who\n";
        let new = "issue:\n  types: [Bug, Feature]\n";
        assert_eq!(patched(existing, new).unwrap(), "# a\nissue.types: [Bug, Feature]\n");
    }

    #[test]
    fn a_field_left_under_a_second_spelling_is_refused() {
        // `project.id` still holds the old name next to the patched-in `project.name`.
        let existing = "project:\n  id: Old\n";
        assert_eq!(patch(existing, "project:\n  name: Old\n", "project:\n  name: New\n", |_| true), None);
    }

    #[test]
    fn a_comment_between_list_items_stays() {
        let existing = "# top\nissue:\n  types:\n  - Bug\n  # chore next\n  - Chore # keep\n  - Spike\n  # trailing\nagent: {}\n";
        let new = "issue:\n  types: [Bug, Chore, Feature]\nagent: {}\n";
        assert_eq!(
            patched(existing, new).unwrap(),
            "# top\nissue:\n  types:\n  - Bug\n  # chore next\n  - Chore # keep\n  - Feature\n  # trailing\nagent: {}\n"
        );
    }

    #[test]
    fn a_reordered_list_is_rewritten_under_its_key_line() {
        let existing = "# top\nissue:\n  types: # why\n  - Bug\n  # note\n  - Chore\n";
        let new = "issue:\n  types: [Chore, Bug]\n";
        assert_eq!(patched(existing, new).unwrap(), "# top\nissue:\n  types: # why\n  - Chore\n  - Bug\n");
    }

    #[test]
    fn a_column_zero_comment_inside_a_section_stays() {
        let existing = "agent:\n  worktree:\n# owner: 3\n    max_parallel_jobs: 3 # owner\n    waiting_run: [cargo]\n";
        let new = "agent:\n  worktree:\n    max_parallel_jobs: 4\n    waiting_run: [cargo]\n";
        assert_eq!(patched(existing, new).unwrap(), "agent:\n  worktree:\n# owner: 3\n    max_parallel_jobs: 4 # owner\n    waiting_run: [cargo]\n");
    }

    #[test]
    fn split_comment_ignores_hashes_in_quotes() {
        assert_eq!(split_comment(" 'a # b' # c"), ("'a # b'".to_string(), Some("# c".to_string())));
        assert_eq!(split_comment(" a#b"), ("a#b".to_string(), None));
    }
}
