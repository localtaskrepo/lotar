use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Page {
    pub limit: usize,
    pub offset: usize,
}

pub fn parse_page(
    query: &HashMap<String, String>,
    default_limit: usize,
    max_limit: usize,
) -> Result<Page, String> {
    let limit_raw = query
        .get("limit")
        .or_else(|| query.get("page_size"))
        .or_else(|| query.get("per_page"));
    let offset_raw = query.get("offset");

    let limit = match limit_raw {
        Some(v) if !v.trim().is_empty() => v
            .trim()
            .parse::<usize>()
            .map_err(|_| format!("Invalid limit: {v}"))?,
        _ => default_limit,
    };

    let offset = match offset_raw {
        Some(v) if !v.trim().is_empty() => v
            .trim()
            .parse::<usize>()
            .map_err(|_| format!("Invalid offset: {v}"))?,
        _ => 0,
    };

    if max_limit == 0 {
        return Err("Invalid max_limit".into());
    }

    let limit = limit.clamp(1, max_limit);
    Ok(Page { limit, offset })
}

pub fn slice_bounds(total: usize, offset: usize, limit: usize) -> (usize, usize) {
    let start = offset.min(total);
    let end = start.saturating_add(limit).min(total);
    (start, end)
}
