use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Rule {
    pub pattern: String,
    pub owners: Vec<String>,
    pub anchored: bool,
}

#[derive(Debug, Clone)]
pub struct CodeOwners {
    pub rules: Vec<Rule>,
}

impl CodeOwners {
    pub fn load_from_repo(repo_root: &Path) -> Option<Self> {
        let candidates = [
            repo_root.join(".github").join("CODEOWNERS"),
            repo_root.join("CODEOWNERS"),
            repo_root.join("docs").join("CODEOWNERS"),
            repo_root.join(".gitlab").join("CODEOWNERS"),
        ];
        let path = candidates.iter().find(|p| p.is_file())?.to_path_buf();
        let content = fs::read_to_string(&path).ok()?;
        let rules = parse_rules(&content);
        Some(CodeOwners { rules })
    }

    pub fn owners_for_path(&self, path: &str) -> Vec<String> {
        let mut matched: Option<&Rule> = None;
        for rule in &self.rules {
            if pattern_matches(&rule.pattern, rule.anchored, path) {
                matched = Some(rule);
            }
        }
        if let Some(rule) = matched {
            rule.owners
                .iter()
                .map(|o| o.trim_start_matches('@').to_string())
                .collect()
        } else {
            Vec::new()
        }
    }

    pub fn default_owner(&self) -> Option<String> {
        let root_match = self.owners_for_path("/");
        if let Some(first) = root_match.first() {
            return Some(first.clone());
        }
        let star = self.owners_for_path("any/path.ext");
        star.first().cloned()
    }
}

fn parse_rules(content: &str) -> Vec<Rule> {
    let mut rules = Vec::new();
    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        if let Some(pattern) = parts.next() {
            let owners: Vec<String> = parts.map(|s| s.to_string()).collect();
            if owners.is_empty() {
                continue;
            }
            let anchored = pattern.starts_with('/');
            rules.push(Rule {
                pattern: pattern.to_string(),
                owners,
                anchored,
            });
        }
    }
    rules
}

fn pattern_matches(pattern: &str, anchored: bool, path: &str) -> bool {
    let mut regex_str = String::from("^");
    let pat_bytes = pattern.as_bytes();
    let mut i = 0;
    while i < pat_bytes.len() {
        match pat_bytes[i] as char {
            '.' | '+' | '(' | ')' | '|' | '^' | '$' | '{' | '}' | '[' | ']' | '\\' => {
                regex_str.push('\\');
                regex_str.push(pat_bytes[i] as char);
                i += 1;
            }
            '/' => {
                regex_str.push('/');
                i += 1;
            }
            '*' => {
                if i + 1 < pat_bytes.len() && pat_bytes[i + 1] as char == '*' {
                    regex_str.push_str(".*");
                    i += 2;
                } else {
                    regex_str.push_str("[^/]*");
                    i += 1;
                }
            }
            '?' => {
                regex_str.push_str("[^/]");
                i += 1;
            }
            c => {
                regex_str.push(c);
                i += 1;
            }
        }
    }
    if !anchored {
        regex_str = format!(".*{}", regex_str.trim_start_matches('^'));
    }
    regex_str.push('$');

    let re = match regex::Regex::new(&regex_str) {
        Ok(r) => r,
        Err(_) => return false,
    };

    re.is_match(path)
}

pub fn repo_root_from_tasks_root(tasks_root: &Path) -> Option<PathBuf> {
    let start = tasks_root.parent().unwrap_or(tasks_root);
    crate::utils::git::find_repo_root(start)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_match_simple() {
        let rules = parse_rules(
            r#"
# Comments ignored
*       @team/default
/src/** @alice @bob
docs/**  @docs
"#,
        );
        let cfg = CodeOwners { rules };
        assert_eq!(cfg.owners_for_path("/src/lib.rs")[0], "alice");
        assert_eq!(cfg.owners_for_path("/README.md")[0], "team/default");
        assert_eq!(cfg.owners_for_path("docs/guide.md")[0], "docs");
        assert_eq!(cfg.default_owner().unwrap(), "team/default");
    }
}
