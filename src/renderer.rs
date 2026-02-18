use crate::diff::{extract_item_properties, CategoryDiff, DiffReport};
use serde_json::Value;
use std::collections::HashMap;

// =============================================================================
// Diff rendering
// =============================================================================

/// Render a diff page in Confluence Wiki Markup format.
/// Returns (page_title, page_body).
pub fn render_diff_page(
    before_label: Option<&str>,
    after_label: Option<&str>,
    report: &DiffReport,
) -> (String, String) {
    let before = before_label.unwrap_or("before");
    let after = after_label.unwrap_or("after");
    let title = format!("Diff: {} \u{2192} {}", before, after);

    let mut body = String::new();

    // Header info panel
    body.push_str(&render_diff_header(before, after));

    // Summary table
    body.push_str(&render_summary_table(report));

    // Per-category sections (sorted alphabetically)
    let mut categories: Vec<_> = report.iter().collect();
    categories.sort_by_key(|(name, _)| name.to_lowercase());

    for (category_name, diff) in &categories {
        body.push_str(&render_category(category_name, diff));
    }

    (title, body)
}

fn render_diff_header(before: &str, after: &str) -> String {
    let mut out = String::new();
    out.push_str("{info}\n");
    out.push_str(&format!("*Before:* {}\n", escape_wiki(before)));
    out.push_str(&format!("*After:* {}\n", escape_wiki(after)));
    out.push_str("{info}\n\n");
    out.push_str("----\n\n");
    out
}

fn render_summary_table(report: &DiffReport) -> String {
    let mut out = String::new();
    out.push_str("h2. Summary\n\n");

    out.push_str("||Category||Added||Removed||Changed||\n");

    let mut categories: Vec<_> = report.iter().collect();
    categories.sort_by_key(|(name, _)| name.to_lowercase());

    for (name, diff) in &categories {
        out.push_str(&format!(
            "|*{}*|{}|{}|{}|\n",
            capitalize(&escape_wiki(name)),
            status_badge("Added", diff.added.len(), "Green"),
            status_badge("Removed", diff.removed.len(), "Red"),
            status_badge("Changed", diff.changed.len(), "Yellow"),
        ));
    }

    out.push('\n');
    out
}

fn render_category(name: &str, diff: &CategoryDiff) -> String {
    let mut out = String::new();
    let display_name = capitalize(&escape_wiki(name));

    out.push_str(&format!("h2. {}\n\n", display_name));

    out.push_str(&render_added_section(&diff.added));
    out.push_str(&render_removed_section(&diff.removed));
    out.push_str(&render_changed_section(&diff.changed));

    out
}

fn render_added_section(items: &[Value]) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "h3. {} Added\n\n",
        status_lozenge(items.len(), "Green"),
    ));

    if items.is_empty() {
        out.push_str("_No additions._\n\n");
        return out;
    }

    out.push_str(&render_item_table(items));
    out
}

fn render_removed_section(items: &[Value]) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "h3. {} Removed\n\n",
        status_lozenge(items.len(), "Red"),
    ));

    if items.is_empty() {
        out.push_str("_No removals._\n\n");
        return out;
    }

    out.push_str(&render_item_table(items));
    out
}

fn render_changed_section(items: &[crate::diff::ChangedItem]) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "h3. {} Changed\n\n",
        status_lozenge(items.len(), "Yellow"),
    ));

    if items.is_empty() {
        out.push_str("_No changes._\n\n");
        return out;
    }

    out.push_str("||Code||Field||Old Value||New Value||\n");

    for item in items {
        for change in &item.changes {
            out.push_str(&format!(
                "|{{{{{}}}}}|{{{{{}}}}}|{color_red}{}{color_end}|{color_green}{}{color_end}|\n",
                escape_wiki_cell(&item.code),
                escape_wiki_cell(&change.field_path),
                escape_wiki_cell(&change.old),
                escape_wiki_cell(&change.new),
                color_red = "{color:red}",
                color_end = "{color}",
                color_green = "{color:green}",
            ));
        }
    }

    out.push('\n');
    out
}

/// Render a table of added/removed items using their extracted properties.
fn render_item_table(items: &[Value]) -> String {
    let all_props: Vec<Vec<(String, String)>> =
        items.iter().map(|i| extract_item_properties(i)).collect();

    // Determine unique column names, preserving insertion order
    let mut columns: Vec<String> = Vec::new();
    for props in &all_props {
        for (key, _) in props {
            if !columns.contains(key) {
                columns.push(key.clone());
            }
        }
    }

    let mut out = String::new();

    // Header row
    out.push_str("||");
    for col in &columns {
        out.push_str(&capitalize(&escape_wiki(col)));
        out.push_str("||");
    }
    out.push('\n');

    // Data rows
    for props in &all_props {
        let prop_map: HashMap<&str, &str> = props
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        out.push('|');
        for col in &columns {
            let val = prop_map.get(col.as_str()).unwrap_or(&"\u{2014}");
            if col == "code" {
                out.push_str(&format!("{{{{{}}}}}", escape_wiki_cell(val)));
            } else {
                out.push_str(&escape_wiki_cell(val));
            }
            out.push('|');
        }
        out.push('\n');
    }

    out.push('\n');
    out
}

// =============================================================================
// Snapshot rendering
// =============================================================================

/// Render a snapshot page in Confluence Wiki Markup format.
/// The snapshot data is expected to be a JSON object where each key is a category
/// name and each value is an array of items.
/// Returns (page_title, page_body).
pub fn render_snapshot_page(label: Option<&str>, data: &Value) -> (String, String) {
    let display_label = label.unwrap_or("Unnamed snapshot");
    let title = format!("Snapshot: {}", display_label);

    let mut body = String::new();

    // Header
    body.push_str("{info}\n");
    body.push_str(&format!("*Snapshot:* {}\n", escape_wiki(display_label)));
    body.push_str("{info}\n\n");
    body.push_str("----\n\n");

    let Some(obj) = data.as_object() else {
        body.push_str("_No data available._\n");
        return (title, body);
    };

    // Summary: count items per category
    body.push_str("h2. Summary\n\n");
    body.push_str("||Category||Items||\n");

    let mut categories: Vec<_> = obj.iter().collect();
    categories.sort_by_key(|(name, _)| name.to_lowercase());

    for (name, value) in &categories {
        let count = value.as_array().map(|a| a.len()).unwrap_or(0);
        body.push_str(&format!(
            "|*{}*|{}|\n",
            capitalize(&escape_wiki(name)),
            count,
        ));
    }
    body.push('\n');

    // Per-category item tables
    for (name, value) in &categories {
        let display_name = capitalize(&escape_wiki(name));
        body.push_str(&format!("h2. {}\n\n", display_name));

        if let Some(items) = value.as_array() {
            if items.is_empty() {
                body.push_str("_No items._\n\n");
                continue;
            }
            body.push_str(&render_item_table(items));
        } else {
            body.push_str("_Invalid data format._\n\n");
        }
    }

    (title, body)
}

// =============================================================================
// Formatting helpers
// =============================================================================

fn status_badge(label: &str, count: usize, color: &str) -> String {
    if count == 0 {
        return format!("{{status:title={}: 0|colour=Grey}}", label);
    }
    format!("{{status:title={}: {}|colour={}}}", label, count, color)
}

fn status_lozenge(count: usize, color: &str) -> String {
    format!("{{status:title={}|colour={}}}", count, color)
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

/// Escape characters that have special meaning in Confluence wiki markup.
fn escape_wiki(s: &str) -> String {
    // In general wiki markup text, we escape braces and brackets
    s.replace('{', "\\{")
        .replace('}', "\\}")
        .replace('[', "\\[")
        .replace(']', "\\]")
}

/// Escape characters inside table cells. Pipes must also be escaped
/// to avoid breaking the table structure.
fn escape_wiki_cell(s: &str) -> String {
    s.replace('{', "\\{")
        .replace('}', "\\}")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace('|', "\\|")
}
