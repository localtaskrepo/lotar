//! Validation of the web UI branding settings (`ui.*`, DEV-218, DEV-220).
//! Reading and serving the images and skins is `services::branding_service`
//! and `services::theme_file`.

use std::path::{Component, Path};

/// Skins the web UI ships and their colour palettes, classic then playful
/// (keep in sync with `THEMES` in `view/utils/themes.ts`;
/// `view/__tests__/themes.spec.ts` checks this). The first palette is the
/// skin's default; an empty list means one fixed palette. `ui.theme` is
/// written `skin` or `skin:palette`.
pub const UI_THEMES: &[(&str, &[&str])] = &[
    ("classic", &["lotar", "ocean", "forest", "ember", "violet", "rose"]),
    ("contrast", &[]),
    ("terminal", &["amber", "green", "red", "cyan", "white"]),
    ("matrix", &["green", "red", "blue"]),
    ("synthwave", &["sunset", "outrun", "ice"]),
    ("blueprint", &["blue", "green", "sepia"]),
    ("paper", &["blue", "black", "red", "green"]),
    ("retro", &["teal", "plum", "slate", "desert"]),
    ("candy", &["bubblegum", "mint", "grape"]),
];

/// Theme ids from before palettes (DEV-218/219), still accepted; they map to
/// classic palettes (`LEGACY_THEME_ALIASES` in `view/utils/themes.ts`).
pub const UI_THEME_ALIASES: &[(&str, &str)] = &[
    ("lotar", "classic"),
    ("ocean", "classic:ocean"),
    ("forest", "classic:forest"),
    ("ember", "classic:ember"),
    ("violet", "classic:violet"),
    ("rose", "classic:rose"),
];

/// `ui.theme` value selecting the repository's own skin file.
pub const CUSTOM_UI_THEME: &str = "custom";

/// Check a `ui.theme` value: `skin`, `skin:palette` with a known palette, a
/// legacy id, or `custom`. Surrounding whitespace is ignored.
pub fn check_ui_theme(value: &str) -> Result<(), String> {
    let value = value.trim();
    if value == CUSTOM_UI_THEME {
        return Ok(());
    }
    let value = UI_THEME_ALIASES.iter().find(|(alias, _)| *alias == value).map_or(value, |(_, target)| *target);
    let (skin, palette) = match value.split_once(':') {
        Some((skin, palette)) => (skin, Some(palette)),
        None => (value, None),
    };
    let Some((_, palettes)) = UI_THEMES.iter().find(|(id, _)| *id == skin) else {
        let skins: Vec<&str> = UI_THEMES.iter().map(|(id, _)| *id).collect();
        return Err(format!("Unknown theme '{value}' (expected a theme such as classic or terminal:red, or {CUSTOM_UI_THEME}; themes: {})", skins.join(", ")));
    };
    match palette {
        None => Ok(()),
        Some(palette) if palettes.contains(&palette) => Ok(()),
        Some(palette) if palettes.is_empty() => Err(format!("Theme '{skin}' has no colour palettes (got '{palette}')")),
        Some(palette) => Err(format!("Unknown palette '{palette}' for theme '{skin}' (expected one of: {})", palettes.join(", "))),
    }
}

/// Whether `value` is an accepted `ui.theme` (see [`check_ui_theme`]).
pub fn is_valid_ui_theme(value: &str) -> bool {
    check_ui_theme(value).is_ok()
}

/// Longest display name shown next to the logo.
pub const MAX_UI_NAME_CHARS: usize = 40;

/// Extensions accepted for logos and icons.
pub const IMAGE_EXTENSIONS: &[&str] = &["svg", "png", "jpg", "jpeg", "webp", "gif", "ico", "avif"];

/// Validate a `ui_*` config value. Empty values clear a setting and are
/// always accepted.
pub fn validate_ui_field(field: &str, value: &str) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(());
    }
    match field {
        "ui_theme" => check_ui_theme(value),
        "ui_name" => {
            if value.chars().count() > MAX_UI_NAME_CHARS {
                Err(format!("ui.name is too long (at most {MAX_UI_NAME_CHARS} characters)"))
            } else if value.chars().any(char::is_control) {
                Err("ui.name cannot contain control characters".to_string())
            } else {
                Ok(())
            }
        }
        "ui_logo" | "ui_logo_dark" | "ui_icon" => validate_image_path(value),
        _ => Ok(()),
    }
}

/// A branding image path must be relative, stay inside the workspace, and
/// name an image file.
pub fn validate_image_path(value: &str) -> Result<(), String> {
    let path = Path::new(value);
    if path.is_absolute() || value.starts_with('~') {
        return Err(format!("'{value}' must be a path relative to the repository (for example assets/logo.svg)"));
    }
    if path.components().any(|component| !matches!(component, Component::Normal(_) | Component::CurDir)) {
        return Err(format!("'{value}' must stay inside the repository"));
    }
    let extension = path.extension().and_then(|ext| ext.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    if !IMAGE_EXTENSIONS.contains(&extension.as_str()) {
        return Err(format!("'{value}' is not an image (expected {})", IMAGE_EXTENSIONS.join(", ")));
    }
    Ok(())
}

/// `ui.theme_file` must be a relative `.css` path inside the repository.
pub fn validate_theme_file_path(value: &str) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(());
    }
    let path = Path::new(value);
    if path.is_absolute() || value.starts_with('~') {
        return Err(format!("'{value}' must be a path relative to the repository (for example assets/lotar-theme.css)"));
    }
    if path.components().any(|component| !matches!(component, Component::Normal(_) | Component::CurDir)) {
        return Err(format!("'{value}' must stay inside the repository"));
    }
    let is_css = path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| ext.eq_ignore_ascii_case("css"));
    if !is_css {
        return Err(format!("'{value}' is not a CSS file (expected .css)"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_settings() {
        assert!(validate_ui_field("ui_theme", "").is_ok());
        assert!(validate_ui_field("ui_theme", "neon").is_err());
        assert!(validate_ui_field("ui_name", "Acme Tracker").is_ok());
        assert!(validate_ui_field("ui_name", &"x".repeat(41)).is_err());
        assert!(validate_ui_field("ui_logo", "assets/logo.svg").is_ok());
        assert!(validate_ui_field("ui_logo", "./assets/logo.PNG").is_ok());
        for bad in ["/etc/logo.png", "~/logo.png", "../logo.png", "assets/../../x.png", "assets/notes.txt", "assets/logo"] {
            assert!(validate_ui_field("ui_icon", bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn validates_themes_and_palettes() {
        for ok in [
            "classic",
            "classic:lotar",
            "classic:ocean",
            "terminal",
            "terminal:red",
            " terminal:amber ",
            "contrast",
            "custom",
            // Legacy ids from before palettes.
            "lotar",
            "forest",
            "rose",
        ] {
            assert!(validate_ui_field("ui_theme", ok).is_ok(), "{ok}");
            assert!(is_valid_ui_theme(ok), "{ok}");
        }
        for bad in ["neon", "terminal:pink", "terminal:", "contrast:dark", "custom:red", "forest:ember", "classic:ocean:x", ":red"] {
            assert!(validate_ui_field("ui_theme", bad).is_err(), "{bad}");
        }
        let err = check_ui_theme("terminal:pink").unwrap_err();
        assert!(err.contains("amber, green, red, cyan, white"), "{err}");
        let err = check_ui_theme("neon").unwrap_err();
        assert!(err.contains("classic, contrast, terminal"), "{err}");
    }

    #[test]
    fn theme_aliases_point_at_real_palettes() {
        for (alias, target) in UI_THEME_ALIASES {
            assert!(UI_THEMES.iter().all(|(skin, _)| skin != alias), "{alias}");
            assert!(is_valid_ui_theme(target), "{alias} -> {target}");
        }
        for (skin, palettes) in UI_THEMES {
            assert!(!skin.contains(':') && *skin != CUSTOM_UI_THEME, "{skin}");
            assert_ne!(palettes.len(), 1, "{skin}: a single palette should be none");
        }
    }

    #[test]
    fn validates_paths() {
        assert!(validate_theme_file_path("assets/lotar-theme.css").is_ok());
        assert!(validate_theme_file_path("").is_ok());
        for bad in ["/etc/x.css", "~/x.css", "../x.css", "assets/x.scss", "x"] {
            assert!(validate_theme_file_path(bad).is_err(), "{bad}");
        }
    }
}
