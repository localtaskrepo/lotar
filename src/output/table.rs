use comfy_table::{Cell, ContentArrangement, Table};

use super::Outputable;

pub fn render_table_single<T: Outputable>(item: &T) -> String {
    let mut table = Table::new();
    table.set_content_arrangement(ContentArrangement::Dynamic);

    let headers = T::table_headers();
    let values = item.to_table_row();

    for (header, value) in headers.iter().zip(values.iter()) {
        table.add_row(vec![Cell::new(header), Cell::new(value)]);
    }

    table.to_string()
}

pub fn render_table_list<T: Outputable>(items: &[T], title: Option<&str>) -> String {
    use console::style;
    let mut output = String::new();

    if let Some(title) = title {
        output.push_str(&style(title).bold().underlined().to_string());
        output.push_str("\n\n");
    }

    if items.is_empty() {
        return format!("{}No items found.", output);
    }

    let mut table = Table::new();
    table.set_content_arrangement(ContentArrangement::Dynamic);

    // Add headers
    let headers = T::table_headers();
    table.set_header(headers);

    // Add rows
    for item in items {
        table.add_row(item.to_table_row());
    }

    output.push_str(&table.to_string());
    output
}
