use std::fmt;
use std::path::{Path, PathBuf};

/// Identity resolution source
#[derive(Clone, Debug)]
pub enum IdentitySource {
    ConfigDefaultReporter,
    ProjectManifestAuthor,
    GitUserName,
    GitUserEmail,
    EnvUser,
    EnvUsername,
}

impl fmt::Display for IdentitySource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IdentitySource::ConfigDefaultReporter => write!(f, "config.default_reporter"),
            IdentitySource::ProjectManifestAuthor => write!(f, "project manifest author"),
            IdentitySource::GitUserName => write!(f, "git user.name"),
            IdentitySource::GitUserEmail => write!(f, "git user.email"),
            IdentitySource::EnvUser => write!(f, "$USER"),
            IdentitySource::EnvUsername => write!(f, "$USERNAME"),
        }
    }
}

/// Result of a detector run
#[derive(Clone, Debug)]
pub struct IdentityDetection {
    pub user: String,
    pub source: IdentitySource,
    /// 0-100 rough confidence indicator; higher is more authoritative
    pub confidence: u8,
    /// Optional diagnostic details to aid explain output
    pub details: Option<String>,
}

/// Detector context passed to detectors
#[derive(Clone, Debug)]
pub struct DetectContext<'a> {
    pub tasks_root: Option<&'a Path>,
}

pub trait IdentityDetector {
    fn detect(&self, ctx: &DetectContext) -> Option<IdentityDetection>;
}

/// Detect from merged configuration default_reporter
pub struct ConfigDefaultReporterDetector;

impl IdentityDetector for ConfigDefaultReporterDetector {
    fn detect(&self, ctx: &DetectContext) -> Option<IdentityDetection> {
        let cfg = match ctx.tasks_root {
            Some(root) => crate::config::resolution::load_and_merge_configs(Some(root)),
            None => crate::config::resolution::load_and_merge_configs(None),
        }
        .ok()?;

        let rep = cfg.default_reporter.and_then(|s| {
            let t = s.trim().to_string();
            if t.is_empty() { None } else { Some(t) }
        })?;

        Some(IdentityDetection {
            user: rep,
            source: IdentitySource::ConfigDefaultReporter,
            confidence: 100,
            details: None,
        })
    }
}

/// Detect from local git config (prefer user.name then user.email)
pub struct GitConfigDetector;

impl GitConfigDetector {
    fn git_context(tasks_root: Option<&Path>) -> Option<(PathBuf, PathBuf)> {
        let start = tasks_root
            .and_then(|root| root.parent().map(|p| p.to_path_buf()))
            .or_else(|| std::env::current_dir().ok())?;
        let repo_root = crate::utils::git::find_repo_root(&start)?;
        let config = repo_root.join(".git").join("config");
        Some((repo_root, config))
    }
}

impl IdentityDetector for GitConfigDetector {
    fn detect(&self, ctx: &DetectContext) -> Option<IdentityDetection> {
        let (repo_root, config_path) = Self::git_context(ctx.tasks_root)?;
        if !config_path.exists() {
            return None;
        }
        let contents = std::fs::read_to_string(&config_path).ok()?;
        let branch = crate::utils::git::read_current_branch(&repo_root);
        let remotes = crate::utils::git::read_remotes(&repo_root);

        // user.name
        for line in contents.lines() {
            let line = line.trim();
            if line.starts_with("name = ") {
                let name = line.trim_start_matches("name = ").trim();
                if !name.is_empty() {
                    let mut details = format!("read from {}", config_path.to_string_lossy());
                    if let Some(b) = &branch {
                        details.push_str(&format!(", branch: {}", b));
                    }
                    if !remotes.is_empty() {
                        details.push_str(&format!(", remotes: {}", remotes.join(";")));
                    }
                    return Some(IdentityDetection {
                        user: name.to_string(),
                        source: IdentitySource::GitUserName,
                        confidence: 85,
                        details: Some(details),
                    });
                }
            }
        }

        // user.email
        for line in contents.lines() {
            let line = line.trim();
            if line.starts_with("email = ") {
                let email = line.trim_start_matches("email = ").trim();
                if !email.is_empty() {
                    let mut details = format!("read from {}", config_path.to_string_lossy());
                    if let Some(b) = &branch {
                        details.push_str(&format!(", branch: {}", b));
                    }
                    if !remotes.is_empty() {
                        details.push_str(&format!(", remotes: {}", remotes.join(";")));
                    }
                    return Some(IdentityDetection {
                        user: email.to_string(),
                        source: IdentitySource::GitUserEmail,
                        confidence: 80,
                        details: Some(details),
                    });
                }
            }
        }
        None
    }
}

/// Detect from common project manifests (package.json, Cargo.toml, .csproj)
pub struct ProjectManifestDetector;

impl ProjectManifestDetector {
    fn search_roots(tasks_root: Option<&Path>) -> Vec<PathBuf> {
        let mut roots = Vec::new();
        // Prefer repo root if available
        if let Some(start) = tasks_root
            .and_then(|root| root.parent().map(|p| p.to_path_buf()))
            .or_else(|| std::env::current_dir().ok())
        {
            if let Some(repo) = crate::utils::git::find_repo_root(&start) {
                roots.push(repo);
            }
            // Also check the immediate start dir in case not at repo root
            roots.push(start);
        }
        // Dedup while preserving order
        roots.dedup();
        roots
    }

    fn parse_package_json(path: &Path) -> Option<String> {
        let contents = std::fs::read_to_string(path).ok()?;
        let json: serde_json::Value = serde_json::from_str(&contents).ok()?;
        // author can be string or object
        if let Some(author) = json.get("author") {
            if author.is_string() {
                let s = author.as_str()?.trim();
                if !s.is_empty() {
                    return Some(s.to_string());
                }
            } else if let Some(name) = author.get("name").and_then(|v| v.as_str()) {
                let s = name.trim();
                if !s.is_empty() {
                    return Some(s.to_string());
                }
            } else if let Some(email) = author.get("email").and_then(|v| v.as_str()) {
                let s = email.trim();
                if !s.is_empty() {
                    return Some(s.to_string());
                }
            }
        }
        // contributors array fallback
        if let Some(contribs) = json.get("contributors").and_then(|v| v.as_array()) {
            if let Some(first) = contribs.first() {
                if first.is_string() {
                    let s = first.as_str()?.trim();
                    if !s.is_empty() {
                        return Some(s.to_string());
                    }
                } else if let Some(name) = first.get("name").and_then(|v| v.as_str()) {
                    let s = name.trim();
                    if !s.is_empty() {
                        return Some(s.to_string());
                    }
                } else if let Some(email) = first.get("email").and_then(|v| v.as_str()) {
                    let s = email.trim();
                    if !s.is_empty() {
                        return Some(s.to_string());
                    }
                }
            }
        }
        None
    }

    fn parse_cargo_toml(path: &Path) -> Option<String> {
        let contents = std::fs::read_to_string(path).ok()?;
        // naive match authors = ["Name <email>", ...]
        // Try find the first quoted string inside authors array
        let mut in_authors = false;
        for line in contents.lines() {
            let l = line.trim();
            if l.starts_with("authors") && l.contains('[') {
                in_authors = true;
                // try inline array
                if let Some(start) = l.find('[') {
                    if let Some(end) = l[start + 1..].find(']') {
                        let inner = &l[start + 1..start + 1 + end];
                        if let Some(val) = Self::first_quoted(inner) {
                            return Some(Self::extract_name_or_email(&val));
                        }
                        in_authors = false;
                    }
                }
                continue;
            }
            if in_authors {
                if l.contains(']') {
                    in_authors = false;
                }
                if let Some(val) = Self::first_quoted(l) {
                    return Some(Self::extract_name_or_email(&val));
                }
            }
        }
        None
    }

    fn parse_csproj(path: &Path) -> Option<String> {
        let contents = std::fs::read_to_string(path).ok()?;
        // minimal XML tag search for <Authors>...</Authors>
        if let Some(start) = contents.find("<Authors>") {
            if let Some(end) = contents[start + 9..].find("</Authors>") {
                let inner = &contents[start + 9..start + 9 + end];
                let s = inner.trim();
                if !s.is_empty() {
                    return Some(s.to_string());
                }
            }
        }
        None
    }

    fn first_quoted(s: &str) -> Option<String> {
        let mut start = None;
        for (i, ch) in s.char_indices() {
            if ch == '"' {
                if start.is_none() {
                    start = Some(i + 1);
                } else {
                    let Some(st) = start else { break };
                    return Some(s[st..i].to_string());
                }
            }
        }
        None
    }

    fn extract_name_or_email(s: &str) -> String {
        // If string like "Name <email>", return Name; else entire string
        if let Some(idx) = s.find('<') {
            return s[..idx].trim().to_string();
        }
        s.trim().to_string()
    }
}

impl IdentityDetector for ProjectManifestDetector {
    fn detect(&self, ctx: &DetectContext) -> Option<IdentityDetection> {
        for root in Self::search_roots(ctx.tasks_root) {
            // package.json
            let pkg = root.join("package.json");
            if pkg.exists() {
                if let Some(author) = Self::parse_package_json(&pkg) {
                    return Some(IdentityDetection {
                        user: author,
                        source: IdentitySource::ProjectManifestAuthor,
                        confidence: 90,
                        details: Some(format!("package.json at {}", pkg.to_string_lossy())),
                    });
                }
            }
            // Cargo.toml
            let cargo = root.join("Cargo.toml");
            if cargo.exists() {
                if let Some(author) = Self::parse_cargo_toml(&cargo) {
                    return Some(IdentityDetection {
                        user: author,
                        source: IdentitySource::ProjectManifestAuthor,
                        confidence: 88,
                        details: Some(format!("Cargo.toml at {}", cargo.to_string_lossy())),
                    });
                }
            }
            // .csproj (any in root)
            if let Ok(entries) = std::fs::read_dir(&root) {
                for ent in entries.flatten() {
                    let p = ent.path();
                    if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                        if ext.eq_ignore_ascii_case("csproj") {
                            if let Some(author) = Self::parse_csproj(&p) {
                                return Some(IdentityDetection {
                                    user: author,
                                    source: IdentitySource::ProjectManifestAuthor,
                                    confidence: 85,
                                    details: Some(format!(".csproj at {}", p.to_string_lossy())),
                                });
                            }
                        }
                    }
                }
            }
        }
        None
    }
}

/// Detect from environment vars
pub struct EnvUserDetector;

impl IdentityDetector for EnvUserDetector {
    fn detect(&self, _ctx: &DetectContext) -> Option<IdentityDetection> {
        if let Ok(user) = std::env::var("USER") {
            let t = user.trim();
            if !t.is_empty() {
                return Some(IdentityDetection {
                    user: t.to_string(),
                    source: IdentitySource::EnvUser,
                    confidence: 60,
                    details: Some("$USER".to_string()),
                });
            }
        }
        if let Ok(user) = std::env::var("USERNAME") {
            let t = user.trim();
            if !t.is_empty() {
                return Some(IdentityDetection {
                    user: t.to_string(),
                    source: IdentitySource::EnvUsername,
                    confidence: 55,
                    details: Some("$USERNAME".to_string()),
                });
            }
        }
        None
    }
}

/// Run detectors in priority order
pub fn detect_identity(ctx: &DetectContext) -> Option<IdentityDetection> {
    // Respect smart toggles
    let cfg = match ctx.tasks_root {
        Some(root) => crate::config::resolution::load_and_merge_configs(Some(root)).ok(),
        None => crate::config::resolution::load_and_merge_configs(None).ok(),
    };

    if let Some(cfg) = cfg.clone() {
        if !cfg.auto_identity {
            // Smart features disabled: honor only configured default_reporter
            let d = ConfigDefaultReporterDetector;
            // Explicitly avoid falling back to git/system if smart is off
            return d.detect(ctx).or(None);
        }
    }

    let mut detectors: Vec<Box<dyn IdentityDetector>> = vec![
        Box::new(ConfigDefaultReporterDetector),
        Box::new(ProjectManifestDetector),
    ];
    // Optionally include git based on toggle
    if cfg.as_ref().map(|c| c.auto_identity_git).unwrap_or(true) {
        detectors.push(Box::new(GitConfigDetector));
    }
    detectors.push(Box::new(EnvUserDetector));

    for d in detectors {
        if let Some(found) = d.detect(ctx) {
            return Some(found);
        }
    }
    None
}
