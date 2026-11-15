use super::Outputable;
use std::fmt::Write;

pub fn render_text_single<T: Outputable>(item: &T) -> String {
    item.to_text()
}

pub fn render_text_list<T: Outputable>(items: &[T], title: Option<&str>, verbose: bool) -> String {
    let mut output = String::new();

    if let Some(title) = title {
        let _ = writeln!(output, "{title}\n");
    }

    if items.is_empty() {
        output.push_str("No items found.");
    } else {
        for (index, item) in items.iter().enumerate() {
            if verbose {
                let _ = writeln!(output, "{}. {}", index + 1, item.to_text());
            } else {
                let _ = writeln!(output, "{}", item.to_text());
            }
        }
    }

    output
}
