use super::Outputable;
use std::fmt::Write;

pub fn render_markdown_single<T: Outputable>(item: &T) -> String {
    let headers = T::table_headers();
    let values = item.to_table_row();

    let mut output = String::new();
    for (header, value) in headers.iter().zip(values.iter()) {
        let _ = writeln!(output, "**{header}:** {value}");
    }
    output
}

pub fn render_markdown_list<T: Outputable>(items: &[T], title: Option<&str>) -> String {
    let mut output = String::new();

    if let Some(title) = title {
        let _ = writeln!(output, "# {title}\n");
    }

    if items.is_empty() {
        output.push_str("No items found.\n");
        return output;
    }

    // Markdown table
    let headers = T::table_headers();
    output.push_str("| ");
    for header in &headers {
        let _ = write!(output, "{header} | ");
    }
    output.push('\n');

    output.push_str("| ");
    for _ in &headers {
        output.push_str("--- | ");
    }
    output.push('\n');

    for item in items {
        let values = item.to_table_row();
        output.push_str("| ");
        for value in values {
            let _ = write!(output, "{value} | ");
        }
        output.push('\n');
    }

    output
}
