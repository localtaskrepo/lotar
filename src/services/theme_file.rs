//! Repository skins (DEV-220): a CSS file in the repository (`ui.theme_file`)
//! that sets the web UI's theme variables, like a theme from
//! `view/themes.css`.
//!
//! Only custom-property declarations (`--name: value`) are kept, re-scoped
//! to `[data-theme-name='custom']` (light) and `…[data-mode='dark']` (any
//! rule whose selector or enclosing `@media` mentions "dark"). Everything
//! else — other properties, `@import`, `@font-face`, `url()` and other
//! resource loads — is dropped, so a shared skin can change how LoTaR looks
//! but cannot load outside resources or restyle the app's internals.

use std::path::{Component, Path, PathBuf};

/// Largest skin file read.
pub const MAX_SKIN_BYTES: u64 = 64 * 1024;
const MAX_DECLARATIONS: usize = 400;
const MAX_VALUE_CHARS: usize = 512;

/// Selector the kept declarations are scoped to.
pub const SKIN_SELECTOR: &str = "[data-theme-name='custom']";

/// `ui.theme_file` must be a relative `.css` path inside the repository.
pub fn validate_path(value: &str) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(());
    }
    let path = Path::new(value);
    if path.is_absolute() || value.starts_with('~') {
        return Err(format!(
            "'{value}' must be a path relative to the repository (for example assets/lotar-theme.css)"
        ));
    }
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return Err(format!("'{value}' must stay inside the repository"));
    }
    let is_css = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("css"));
    if !is_css {
        return Err(format!("'{value}' is not a CSS file (expected .css)"));
    }
    Ok(())
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Skin {
    /// Sanitised CSS, ready to serve.
    pub css: String,
    /// Declarations kept.
    pub kept: usize,
    /// Declarations and rules that were dropped, for the settings page.
    pub dropped: usize,
}

fn strip_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        match rest[start + 2..].find("*/") {
            Some(end) => rest = &rest[start + 2 + end + 2..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// Split `css` into (prelude, body) blocks at the top level, quote-aware.
fn blocks(css: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut prelude = String::new();
    let mut body = String::new();
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    for c in css.chars() {
        if let Some(q) = quote {
            if depth > 0 {
                body.push(c);
            } else {
                prelude.push(c);
            }
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => {
                quote = Some(c);
                if depth > 0 {
                    body.push(c);
                } else {
                    prelude.push(c);
                }
            }
            '{' => {
                if depth > 0 {
                    body.push(c);
                }
                depth += 1;
            }
            '}' => {
                if depth == 0 {
                    continue;
                }
                depth -= 1;
                if depth == 0 {
                    out.push((
                        std::mem::take(&mut prelude).trim().to_string(),
                        std::mem::take(&mut body),
                    ));
                } else {
                    body.push(c);
                }
            }
            // A statement like `@import …;` outside any block.
            ';' if depth == 0 => {
                out.push((
                    std::mem::take(&mut prelude).trim().to_string(),
                    String::new(),
                ));
            }
            _ => {
                if depth > 0 {
                    body.push(c);
                } else {
                    prelude.push(c);
                }
            }
        }
    }
    out
}

fn declarations(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    for c in body.chars() {
        match quote {
            Some(q) => {
                current.push(c);
                if c == q {
                    quote = None;
                }
            }
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                current.push(c);
            }
            None if c == ';' => out.push(std::mem::take(&mut current)),
            None => current.push(c),
        }
    }
    out.push(current);
    out.into_iter()
        .map(|decl| decl.trim().to_string())
        .filter(|decl| !decl.is_empty())
        .collect()
}

fn safe_declaration(decl: &str) -> Option<(String, String)> {
    let (name, value) = decl.split_once(':')?;
    let name = name.trim();
    let value = value.trim().trim_end_matches("!important").trim();
    let name_ok = name.len() > 2
        && name.len() <= 64
        && name.starts_with("--")
        && name[2..]
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !name_ok || value.is_empty() || value.chars().count() > MAX_VALUE_CHARS {
        return None;
    }
    let lower = value.to_ascii_lowercase();
    let forbidden = [
        "url(",
        "image(",
        "image-set(",
        "element(",
        "expression(",
        "javascript:",
    ];
    if forbidden.iter().any(|needle| lower.contains(needle))
        || value
            .chars()
            .any(|c| matches!(c, '{' | '}' | ';' | '<' | '>' | '\\' | '@') || c.is_control())
    {
        return None;
    }
    Some((name.to_string(), value.to_string()))
}

/// Keep only theme variables from `css`, scoped to the custom skin.
pub fn sanitize(css: &str, source: &str) -> Skin {
    let mut light: Vec<(String, String)> = Vec::new();
    let mut dark: Vec<(String, String)> = Vec::new();
    let mut dropped = 0usize;

    let mut visit = |prelude: &str, body: &str, dark_scope: bool, dropped: &mut usize| {
        let is_dark = dark_scope || prelude.to_ascii_lowercase().contains("dark");
        for decl in declarations(body) {
            match safe_declaration(&decl) {
                Some(pair) if light.len() + dark.len() < MAX_DECLARATIONS => {
                    if is_dark {
                        dark.push(pair);
                    } else {
                        light.push(pair);
                    }
                }
                _ => *dropped += 1,
            }
        }
    };

    for (prelude, body) in blocks(&strip_comments(css)) {
        let lower = prelude.to_ascii_lowercase();
        if lower.starts_with("@media") {
            // Only colour-scheme media queries are understood.
            if lower.contains("prefers-color-scheme") {
                let is_dark = lower.contains("dark");
                for (inner_prelude, inner_body) in blocks(&body) {
                    visit(&inner_prelude, &inner_body, is_dark, &mut dropped);
                }
            } else {
                dropped += 1;
            }
        } else if lower.starts_with('@') {
            dropped += 1;
        } else if !body.is_empty() || !prelude.is_empty() {
            visit(&prelude, &body, false, &mut dropped);
        }
    }

    let kept = light.len() + dark.len();
    let render = |selector: &str, decls: &[(String, String)]| -> String {
        if decls.is_empty() {
            return String::new();
        }
        let mut block = format!("{selector} {{\n");
        for (name, value) in decls {
            block.push_str(&format!("  {name}: {value};\n"));
        }
        block.push_str("}\n");
        block
    };
    let source = source.replace("*/", "");
    let css = format!(
        "/* Repository skin from {source}, reduced by LoTaR to theme variables */\n{}{}",
        render(SKIN_SELECTOR, &light),
        render(&format!("{SKIN_SELECTOR}[data-mode='dark']"), &dark)
    );
    Skin { css, kept, dropped }
}

/// Why a configured skin cannot be used.
#[derive(Debug, PartialEq, Eq)]
pub enum SkinError {
    Invalid(String),
    Missing,
    TooLarge,
    NotText,
}

/// Read and sanitise the skin at `relative` inside `root`.
pub fn read_skin(root: &Path, relative: &str) -> Result<(PathBuf, Skin), SkinError> {
    validate_path(relative).map_err(SkinError::Invalid)?;
    let root = root.canonicalize().map_err(|_| SkinError::Missing)?;
    let path = root
        .join(relative)
        .canonicalize()
        .map_err(|_| SkinError::Missing)?;
    // Symlinks may point anywhere; only files really inside the workspace count.
    if !path.starts_with(&root) || !path.is_file() {
        return Err(SkinError::Missing);
    }
    let size = std::fs::metadata(&path)
        .map_err(|_| SkinError::Missing)?
        .len();
    if size > MAX_SKIN_BYTES {
        return Err(SkinError::TooLarge);
    }
    let bytes = std::fs::read(&path).map_err(|_| SkinError::Missing)?;
    let text = String::from_utf8(bytes).map_err(|_| SkinError::NotText)?;
    Ok((path, sanitize(&text, relative)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_theme_variables_and_sorts_out_dark_rules() {
        let skin = sanitize(
            r#"
            /* My skin */
            :root { --color-accent: #c0392b; --font-heading: "Avenir Next", sans-serif; color: red; }
            [data-mode="dark"] { --color-bg: #111; }
            @media (prefers-color-scheme: dark) { :root { --color-fg: #eee; } }
            "#,
            "assets/skin.css",
        );
        assert_eq!(skin.kept, 4);
        assert_eq!(skin.dropped, 1, "plain properties are dropped");
        assert!(skin.css.contains(
            "[data-theme-name='custom'] {\n  --color-accent: #c0392b;\n  --font-heading: \"Avenir Next\", sans-serif;\n}"
        ));
        assert!(skin.css.contains("[data-theme-name='custom'][data-mode='dark'] {\n  --color-bg: #111;\n  --color-fg: #eee;\n}"));
    }

    #[test]
    fn drops_anything_that_could_load_or_escape() {
        let skin = sanitize(
            r#"
            :root { --ok: linear-gradient(90deg, #000, #fff); }
            @import url("https://example.com/x.css");
            @font-face { font-family: X; src: url(x.woff); }
            @media print { :root { --color-bg: #fff; } }
            :root {
              --page-background: url(https://example.com/track.png);
              --a: image-set("x.png" 1x);
              --b: red } body { display: none;
              --c: "x\" ; --d: 1";
            }
            "#,
            "skin.css",
        );
        assert!(!skin.css.contains("url("));
        assert!(!skin.css.contains("@import"));
        assert!(!skin.css.contains("display"));
        assert!(!skin.css.contains("image-set"));
        assert!(
            skin.css
                .contains("--ok: linear-gradient(90deg, #000, #fff);")
        );
        assert!(skin.dropped >= 5, "{skin:?}");
    }

    #[test]
    fn validates_paths() {
        assert!(validate_path("assets/lotar-theme.css").is_ok());
        assert!(validate_path("").is_ok());
        for bad in ["/etc/x.css", "~/x.css", "../x.css", "assets/x.scss", "x"] {
            assert!(validate_path(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn reads_only_files_inside_the_repository() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("repo");
        std::fs::create_dir_all(root.join("assets")).unwrap();
        std::fs::write(
            root.join("assets/skin.css"),
            ":root { --color-accent: #123456; }",
        )
        .unwrap();
        let (_, skin) = read_skin(&root, "assets/skin.css").unwrap();
        assert_eq!(skin.kept, 1);
        assert_eq!(read_skin(&root, "assets/none.css"), Err(SkinError::Missing));
        std::fs::write(tmp.path().join("outside.css"), ":root { --x: 1; }").unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                tmp.path().join("outside.css"),
                root.join("assets/link.css"),
            )
            .unwrap();
            assert_eq!(read_skin(&root, "assets/link.css"), Err(SkinError::Missing));
        }
    }
}
