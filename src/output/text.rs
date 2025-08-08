use console::style;

use super::Outputable;

pub fn render_text_single<T: Outputable>(item: &T) -> String {
    item.to_text()
}

pub fn render_text_list<T: Outputable>(items: &[T], title: Option<&str>, verbose: bool) -> String {
    let mut output = String::new();

    if let Some(title) = title {
        output.push_str(&format!("{}\n\n", style(title).bold().underlined()));
    }

    if items.is_empty() {
        output.push_str(&style("No items found.").dim().to_string());
    } else {
        for (index, item) in items.iter().enumerate() {
            if verbose {
                output.push_str(&format!("{}. {}\n", index + 1, item.to_text()));
            } else {
                output.push_str(&format!("{}\n", item.to_text()));
            }
        }
    }

    output
}
