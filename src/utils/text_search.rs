//! Free-text task search shared by the CLI, REST API, MCP server and web UI
//! (DEV-167).
//!
//! A query is split into terms that must all match, in any order and in any
//! searched field:
//!
//! - `login redirect` matches tasks containing both words anywhere.
//! - `"login redirect"` matches the exact phrase.
//! - `-oauth` excludes tasks containing `oauth` (also `-"two words"`).
//!
//! Matching ignores case and common Latin accents (`uber` finds `über`), and
//! keeps the old substring behaviour (`redir` finds `redirect`, `ATLS-2` finds
//! the task ID). Words of four or more characters also tolerate typos: one
//! edit, or two from eight characters (`redirct` finds `redirect`). Exclusions
//! and phrases never use typo tolerance.
//!
//! Every match carries a score, so callers can order results by relevance:
//! an exact ID beats an exact title, then a title starting with the query,
//! then matches in the title, subtitle, description and acceptance criteria,
//! tags, and references or comments. Within a field a whole word beats a
//! prefix, an infix or a typo.

use crate::types::{ReferenceEntry, TaskComment};

/// The fields a task is searched in, with how much a match in each counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    Id,
    Title,
    Tag,
    Subtitle,
    Description,
    AcceptanceCriteria,
    Reference,
    Comment,
}

impl Field {
    fn weight(self) -> u32 {
        match self {
            Field::Id => 10,
            Field::Title => 8,
            Field::Subtitle => 5,
            Field::Description | Field::AcceptanceCriteria => 3,
            Field::Tag => 2,
            Field::Reference | Field::Comment => 1,
        }
    }
}

/// How closely a term matched a field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Strength {
    Typo = 1,
    Infix = 2,
    Prefix = 3,
    Word = 4,
}

/// Extra score when the whole query is a task's exact ID.
const EXACT_ID_BONUS: u32 = 1_000;
/// Extra score when the whole query is a task's exact title, or how it starts.
const EXACT_TITLE_BONUS: u32 = 500;
const TITLE_PREFIX_BONUS: u32 = 200;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Term {
    /// Folded text; phrases have their whitespace collapsed to single spaces.
    text: String,
    phrase: bool,
    negated: bool,
}

/// A parsed free-text query.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchQuery {
    terms: Vec<Term>,
    /// The whole query, folded, for the exact-ID bonus.
    whole: String,
}

/// The searchable text of one task. Borrowed, so stored tasks and API task
/// objects can both be scored without copying.
pub struct SearchDocument<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub subtitle: Option<&'a str>,
    pub description: Option<&'a str>,
    pub tags: &'a [String],
    pub acceptance_criteria: &'a [String],
    pub comments: &'a [TaskComment],
    pub references: &'a [ReferenceEntry],
}

/// Tasks that can be searched: stored tasks (CLI, storage filter) and API
/// task objects (REST, MCP) expose the same text.
pub trait Searchable {
    fn search_document<'a>(&'a self, id: &'a str) -> SearchDocument<'a>;
}

impl Searchable for crate::storage::task::Task {
    fn search_document<'a>(&'a self, id: &'a str) -> SearchDocument<'a> {
        SearchDocument {
            id,
            title: &self.title,
            subtitle: self.subtitle.as_deref(),
            description: self.description.as_deref(),
            tags: &self.tags,
            acceptance_criteria: &self.acceptance_criteria,
            comments: &self.comments,
            references: &self.references,
        }
    }
}

impl Searchable for crate::api_types::TaskDTO {
    fn search_document<'a>(&'a self, id: &'a str) -> SearchDocument<'a> {
        SearchDocument {
            id,
            title: &self.title,
            subtitle: self.subtitle.as_deref(),
            description: self.description.as_deref(),
            tags: &self.tags,
            acceptance_criteria: &self.acceptance_criteria,
            comments: &self.comments,
            references: &self.references,
        }
    }
}

impl SearchQuery {
    pub fn parse(raw: &str) -> Self {
        let mut terms = Vec::new();
        let mut chars = raw.chars().peekable();
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() {
                chars.next();
                continue;
            }
            let mut negated = false;
            if c == '-' {
                chars.next();
                match chars.peek() {
                    // `-word` or `-"phrase"` excludes; a lone or doubled dash is text.
                    Some(&next) if next == '"' || next.is_alphanumeric() => negated = true,
                    _ => {
                        let mut text = String::from("-");
                        while let Some(&next) = chars.peek() {
                            if next.is_whitespace() {
                                break;
                            }
                            text.push(next);
                            chars.next();
                        }
                        push_term(&mut terms, &text, false, false);
                        continue;
                    }
                }
            }
            if chars.peek() == Some(&'"') {
                chars.next();
                let mut text = String::new();
                for next in chars.by_ref() {
                    if next == '"' {
                        break;
                    }
                    text.push(next);
                }
                push_term(&mut terms, &text, true, negated);
            } else {
                let mut text = String::new();
                while let Some(&next) = chars.peek() {
                    if next.is_whitespace() {
                        break;
                    }
                    text.push(next);
                    chars.next();
                }
                push_term(&mut terms, &text, false, negated);
            }
        }
        Self {
            terms,
            whole: fold(raw.trim()),
        }
    }

    /// True when the query has nothing to match (blank or only quotes).
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    /// Whether the task matches every term and none of the exclusions.
    pub fn matches(&self, doc: &SearchDocument<'_>) -> bool {
        self.score(doc).is_some()
    }

    /// Relevance of a task for this query, or `None` when it does not match.
    /// An empty query matches everything with a score of 0.
    pub fn score(&self, doc: &SearchDocument<'_>) -> Option<u32> {
        if self.terms.is_empty() {
            return Some(0);
        }
        let fields = folded_fields(doc);
        let mut total = 0;
        for term in &self.terms {
            let best = best_match(term, &fields);
            match (term.negated, best) {
                (true, Some(_)) => return None,
                (true, None) => {}
                (false, None) => return None,
                (false, Some(points)) => total += points,
            }
        }
        if !self.whole.is_empty() {
            if fold(doc.id) == self.whole {
                total += EXACT_ID_BONUS;
            }
            let title = collapse_whitespace(&fold(doc.title));
            let whole = collapse_whitespace(&self.whole);
            if title == whole {
                total += EXACT_TITLE_BONUS;
            } else if title.starts_with(&whole) {
                total += TITLE_PREFIX_BONUS;
            }
        }
        Some(total)
    }
}

fn push_term(terms: &mut Vec<Term>, raw: &str, phrase: bool, negated: bool) {
    let folded = fold(raw);
    let text = if phrase {
        folded.split_whitespace().collect::<Vec<_>>().join(" ")
    } else {
        folded
    };
    if !text.is_empty() {
        terms.push(Term {
            text,
            phrase,
            negated,
        });
    }
}

/// Lowercase text with common Latin accents removed and whitespace runs kept
/// as-is (phrase matching collapses them separately).
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        for lower in c.to_lowercase() {
            match fold_accent(lower) {
                Some(plain) => out.push_str(plain),
                None => out.push(lower),
            }
        }
    }
    out
}

/// Accent-free spelling for lowercase Latin-1 and Latin Extended-A letters.
fn fold_accent(c: char) -> Option<&'static str> {
    Some(match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => "a",
        'æ' => "ae",
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => "c",
        'ď' | 'đ' | 'ð' => "d",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => "e",
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => "g",
        'ĥ' | 'ħ' => "h",
        'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => "i",
        'ĳ' => "ij",
        'ĵ' => "j",
        'ķ' => "k",
        'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => "l",
        'ñ' | 'ń' | 'ņ' | 'ň' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ŏ' | 'ő' => "o",
        'œ' => "oe",
        'ŕ' | 'ŗ' | 'ř' => "r",
        'ś' | 'ŝ' | 'ş' | 'š' => "s",
        'ß' => "ss",
        'ţ' | 'ť' | 'ŧ' => "t",
        'þ' => "th",
        'ù' | 'ú' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => "u",
        'ŵ' => "w",
        'ý' | 'ÿ' | 'ŷ' => "y",
        'ź' | 'ż' | 'ž' => "z",
        _ => return None,
    })
}

struct FoldedField {
    field: Field,
    text: String,
}

fn folded_fields(doc: &SearchDocument<'_>) -> Vec<FoldedField> {
    let mut fields = Vec::with_capacity(4 + doc.tags.len() + doc.comments.len());
    let mut push = |field: Field, text: &str| {
        if !text.trim().is_empty() {
            fields.push(FoldedField {
                field,
                text: fold(text),
            });
        }
    };
    push(Field::Id, doc.id);
    push(Field::Title, doc.title);
    for tag in doc.tags {
        push(Field::Tag, tag);
    }
    if let Some(subtitle) = doc.subtitle {
        push(Field::Subtitle, subtitle);
    }
    if let Some(description) = doc.description {
        push(Field::Description, description);
    }
    for criterion in doc.acceptance_criteria {
        push(Field::AcceptanceCriteria, criterion);
    }
    for reference in doc.references {
        for value in [
            &reference.link,
            &reference.code,
            &reference.file,
            &reference.attachment,
            &reference.jira,
            &reference.github,
        ]
        .into_iter()
        .flatten()
        {
            push(Field::Reference, value);
        }
    }
    for comment in doc.comments {
        push(Field::Comment, &comment.text);
    }
    fields
}

/// The best score a term earns in any field, or `None` when it matches none.
/// Exact and substring matches are checked in every field before falling
/// back to typo tolerance, which is the expensive part.
fn best_match(term: &Term, fields: &[FoldedField]) -> Option<u32> {
    let mut best = None;
    for field in fields {
        let strength = if term.phrase {
            collapse_whitespace(&field.text)
                .contains(&term.text)
                .then_some(Strength::Word)
        } else {
            substring_strength(&field.text, &term.text)
        };
        if let Some(strength) = strength {
            let points = field.field.weight() * strength as u32;
            best = Some(best.map_or(points, |current: u32| current.max(points)));
        }
    }
    if best.is_some() || term.phrase || term.negated {
        return best;
    }
    let max_edits = typo_budget(&term.text)?;
    let term_chars: Vec<char> = term.text.chars().collect();
    fields
        .iter()
        .filter(|field| words(&field.text).any(|word| within_typos(&term_chars, word, max_edits)))
        .map(|field| field.field.weight() * Strength::Typo as u32)
        .max()
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// How the term occurs in the text: as a whole word, at the start of a word,
/// or inside one. The strongest occurrence wins.
fn substring_strength(text: &str, term: &str) -> Option<Strength> {
    let mut best = None;
    for (start, _) in text.match_indices(term) {
        let end = start + term.len();
        let starts_word = !text[..start].chars().next_back().is_some_and(is_word_char);
        let ends_word = !text[end..].chars().next().is_some_and(is_word_char);
        let strength = match (starts_word, ends_word) {
            (true, true) => Strength::Word,
            (true, false) => Strength::Prefix,
            _ => Strength::Infix,
        };
        best = Some(best.map_or(strength, |current: Strength| current.max(strength)));
        if strength == Strength::Word {
            break;
        }
    }
    best
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric()
}

fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !is_word_char(c))
        .filter(|word| !word.is_empty())
}

/// Allowed edits for a term: none below four characters (too many false
/// hits), one up to seven, two from eight. Terms with digits or symbols (IDs,
/// versions, paths) are matched exactly.
fn typo_budget(term: &str) -> Option<usize> {
    if !term.chars().all(char::is_alphabetic) {
        return None;
    }
    match term.chars().count() {
        0..=3 => None,
        4..=7 => Some(1),
        _ => Some(2),
    }
}

/// True when `word`, or the start of it, is within `max_edits` of the term.
/// The prefix check lets a mistyped beginning (`redri`) find a longer word
/// (`redirect`), matching how plain substrings behave.
fn within_typos(term: &[char], word: &str, max_edits: usize) -> bool {
    let word: Vec<char> = word.chars().collect();
    if word.len() + max_edits < term.len() {
        return false;
    }
    if word.len() <= term.len() + max_edits && edit_distance(term, &word, max_edits) <= max_edits {
        return true;
    }
    if word.len() > term.len() {
        // Compare against prefixes around the term's length.
        let shortest = term.len().saturating_sub(max_edits).max(1);
        let longest = (term.len() + max_edits).min(word.len());
        return (shortest..=longest)
            .any(|len| edit_distance(term, &word[..len], max_edits) <= max_edits);
    }
    false
}

/// Optimal string alignment distance (insertions, deletions, substitutions
/// and adjacent transpositions), stopping early once it exceeds `limit`.
fn edit_distance(a: &[char], b: &[char], limit: usize) -> usize {
    if a.len().abs_diff(b.len()) > limit {
        return limit + 1;
    }
    let width = b.len() + 1;
    let mut before: Vec<usize> = vec![0; width];
    let mut previous: Vec<usize> = (0..width).collect();
    let mut current: Vec<usize> = vec![0; width];
    for i in 1..=a.len() {
        current[0] = i;
        let mut row_min = current[0];
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut value = (previous[j] + 1)
                .min(current[j - 1] + 1)
                .min(previous[j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                value = value.min(before[j - 2] + 1);
            }
            current[j] = value;
            row_min = row_min.min(value);
        }
        if row_min > limit {
            return limit + 1;
        }
        std::mem::swap(&mut before, &mut previous);
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc<'a>(
        id: &'a str,
        title: &'a str,
        description: Option<&'a str>,
        tags: &'a [String],
    ) -> SearchDocument<'a> {
        SearchDocument {
            id,
            title,
            subtitle: None,
            description,
            tags,
            acceptance_criteria: &[],
            comments: &[],
            references: &[],
        }
    }

    fn matches(query: &str, document: &SearchDocument<'_>) -> bool {
        SearchQuery::parse(query).matches(document)
    }

    #[test]
    fn words_match_in_any_order_and_any_field() {
        let tags = vec!["auth".to_string()];
        let task = doc(
            "WEB-12",
            "Fix the login redirect loop",
            Some("Happens after OAuth"),
            &tags,
        );
        assert!(matches("redirect login", &task));
        assert!(matches("loop oauth auth", &task));
        assert!(!matches("redirect logout", &task));
    }

    #[test]
    fn keeps_substring_and_id_matching() {
        let task = doc("WEB-12", "Fix the login redirect loop", None, &[]);
        assert!(matches("redir", &task));
        assert!(matches("web-12", &task));
        assert!(matches("EB-1", &task));
        assert!(matches("  ", &task));
    }

    #[test]
    fn phrases_need_the_words_together() {
        let task = doc("WEB-12", "Fix the login   redirect loop", None, &[]);
        assert!(matches("\"login redirect\"", &task));
        assert!(!matches("\"redirect login\"", &task));
        // An unterminated quote runs to the end of the query.
        assert!(matches("\"redirect loop", &task));
    }

    #[test]
    fn exclusions_remove_matches() {
        let task = doc(
            "WEB-12",
            "Fix the login redirect loop",
            Some("OAuth only"),
            &[],
        );
        assert!(!matches("login -oauth", &task));
        assert!(matches("login -saml", &task));
        assert!(!matches("-\"redirect loop\"", &task));
        // A lone or doubled dash is plain text, not an exclusion.
        let flags = doc("CLI-1", "Document the --dry-run flag", None, &[]);
        assert!(matches("--dry-run", &flags));
        let lone = SearchQuery::parse("-");
        assert_eq!(lone.terms.len(), 1);
        assert!(!lone.terms[0].negated);
    }

    #[test]
    fn ignores_case_and_accents() {
        let task = doc("DOC-1", "Über die Straße zum Café", None, &[]);
        assert!(matches("uber strasse cafe", &task));
        assert!(matches("CAFÉ", &task));
    }

    #[test]
    fn tolerates_typos_in_longer_words_only() {
        let task = doc(
            "WEB-12",
            "Fix the login redirect loop",
            Some("Authentication breaks"),
            &[],
        );
        assert!(matches("redirct", &task));
        assert!(matches("rdeirect", &task));
        assert!(matches("athentcation", &task));
        assert!(matches("redri", &task));
        // Short words, numbers and exclusions are exact.
        assert!(!matches("lop", &task));
        assert!(!matches("web-13", &task));
        assert!(matches("login -redirct", &task));
        assert!(!matches("redirect -redirect", &task));
    }

    #[test]
    fn scores_rank_ids_titles_whole_words_and_typos() {
        let query = SearchQuery::parse("login");
        let title = doc("A-1", "Login page", None, &[]);
        let prefix = doc("A-2", "Logins are slow", None, &[]);
        let description = doc("A-3", "Page", Some("The login page"), &[]);
        let typo = doc("A-4", "Lgoin page", None, &[]);
        let scores: Vec<u32> = [&title, &prefix, &description, &typo]
            .iter()
            .map(|task| query.score(task).unwrap())
            .collect();
        assert!(scores[0] > scores[1], "{scores:?}");
        assert!(scores[1] > scores[2], "{scores:?}");
        assert!(scores[2] > scores[3], "{scores:?}");

        let exact = SearchQuery::parse("a-1");
        assert!(exact.score(&title).unwrap() >= EXACT_ID_BONUS);
        assert_eq!(SearchQuery::parse("").score(&title), Some(0));
    }

    #[test]
    fn searches_comments_criteria_and_references() {
        let comments = vec![TaskComment {
            date: "2026-01-01".into(),
            text: "Reproduced on Safari".into(),
        }];
        let criteria = vec!["Works offline".to_string()];
        let references = vec![ReferenceEntry {
            code: Some("web/src/sync/queue.ts#20-33".into()),
            ..Default::default()
        }];
        let task = SearchDocument {
            id: "WEB-1",
            title: "Sync",
            subtitle: None,
            description: None,
            tags: &[],
            acceptance_criteria: &criteria,
            comments: &comments,
            references: &references,
        };
        assert!(matches("safari", &task));
        assert!(matches("offline", &task));
        assert!(matches("queue.ts", &task));
    }

    #[test]
    fn edit_distance_counts_transpositions_once() {
        let chars = |s: &str| s.chars().collect::<Vec<_>>();
        assert_eq!(edit_distance(&chars("redirect"), &chars("redirect"), 2), 0);
        assert_eq!(edit_distance(&chars("redirect"), &chars("rediretc"), 2), 1);
        assert_eq!(edit_distance(&chars("redirect"), &chars("redrect"), 2), 1);
        assert_eq!(edit_distance(&chars("abc"), &chars("xyzabc"), 1), 2);
    }
}
