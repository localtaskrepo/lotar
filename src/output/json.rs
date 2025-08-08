use serde::Serialize;

pub fn render_json_single<T: Serialize>(item: &T, verbose: bool) -> String {
    if verbose {
        serde_json::to_string_pretty(item).unwrap_or_else(|_| "{}".to_string())
    } else {
        serde_json::to_string(item).unwrap_or_else(|_| "{}".to_string())
    }
}

pub fn render_json_list<T: Serialize>(items: &[T], verbose: bool) -> String {
    if verbose {
        serde_json::to_string_pretty(items).unwrap_or_else(|_| "[]".to_string())
    } else {
        serde_json::to_string(items).unwrap_or_else(|_| "[]".to_string())
    }
}
