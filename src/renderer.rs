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
        // Render flat field-level changes (old → new)
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

        // Render nested sub-diffs (added/removed within a field)
        for nested in &item.nested_diffs {
            if !nested.added.is_empty() {
                let added_str = nested
                    .added
                    .iter()
                    .map(|v| escape_wiki_cell(v))
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push_str(&format!(
                    "|{{{{{}}}}}|{{{{{}.added}}}}| |{color_green}{}{color_end}|\n",
                    escape_wiki_cell(&item.code),
                    escape_wiki_cell(&nested.field_path),
                    added_str,
                    color_green = "{color:green}",
                    color_end = "{color}",
                ));
            }
            if !nested.removed.is_empty() {
                let removed_str = nested
                    .removed
                    .iter()
                    .map(|v| escape_wiki_cell(v))
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push_str(&format!(
                    "|{{{{{}}}}}|{{{{{}.removed}}}}|{color_red}{}{color_end}| |\n",
                    escape_wiki_cell(&item.code),
                    escape_wiki_cell(&nested.field_path),
                    removed_str,
                    color_red = "{color:red}",
                    color_end = "{color}",
                ));
            }
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
// Snapshot rendering (multi-page)
// =============================================================================

/// A tree of pages representing a snapshot.
/// The root page ("Current model") contains a summary, and each category
/// gets its own child page.
pub struct SnapshotPageTree {
    pub root_title: String,
    pub root_body: String,
    pub children: Vec<SnapshotChildPage>,
}

/// A single child page for one category in the snapshot.
pub struct SnapshotChildPage {
    pub title: String,
    pub body: String,
}

/// Render a snapshot as a multi-page tree in Confluence Wiki Markup format.
///
/// Returns a `SnapshotPageTree` with:
/// - A root "Current model" page containing a summary table
/// - One child page per root key in the snapshot JSON
///
/// The "families" category gets special treatment: each family is rendered as its
/// own sub-section with a list of attributes belonging to that family.
pub fn render_snapshot_pages(label: Option<&str>, data: &Value) -> SnapshotPageTree {
    let display_label = label.unwrap_or("Unnamed snapshot");
    let root_title = "Current model".to_string();

    let Some(obj) = data.as_object() else {
        return SnapshotPageTree {
            root_title,
            root_body: "_No data available._\n".to_string(),
            children: Vec::new(),
        };
    };

    // Build root page body: info header + summary table
    let mut root_body = String::new();
    root_body.push_str("{info}\n");
    root_body.push_str(&format!("*Snapshot:* {}\n", escape_wiki(display_label)));
    root_body.push_str("{info}\n\n");
    root_body.push_str("----\n\n");

    root_body.push_str("h2. Summary\n\n");
    root_body.push_str("||Category||Items||\n");

    let mut categories: Vec<_> = obj.iter().collect();
    categories.sort_by_key(|(name, _)| name.to_lowercase());

    for (name, value) in &categories {
        let count = value.as_array().map(|a| a.len()).unwrap_or(0);
        root_body.push_str(&format!(
            "|*{}*|{}|\n",
            capitalize(&escape_wiki(name)),
            count,
        ));
    }
    root_body.push('\n');

    // Build child pages, one per category
    let mut children = Vec::new();
    for (name, value) in &categories {
        let page_title = capitalize(name);
        let items = value.as_array().cloned().unwrap_or_default();

        let body = if name.to_lowercase() == "families" {
            render_family_page(&items)
        } else {
            render_category_page(&items)
        };

        children.push(SnapshotChildPage {
            title: page_title,
            body,
        });
    }

    SnapshotPageTree {
        root_title,
        root_body,
        children,
    }
}

/// Render a generic category child page (for non-family categories).
fn render_category_page(items: &[Value]) -> String {
    if items.is_empty() {
        return "_No items._\n\n".to_string();
    }
    render_item_table(items)
}

/// Render the families child page with per-family sub-sections.
///
/// Each family gets:
/// - An h2 heading with the family code
/// - A properties table (code, labels, etc.)
/// - An "Attributes" sub-section listing the attribute codes belonging to that family
fn render_family_page(items: &[Value]) -> String {
    let mut out = String::new();

    if items.is_empty() {
        out.push_str("_No families._\n\n");
        return out;
    }

    for item in items {
        let code = item
            .get("code")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        // Family heading
        out.push_str(&format!("h2. {}\n\n", escape_wiki(code)));

        // Properties table
        let props = extract_item_properties(item);
        if !props.is_empty() {
            out.push_str("||Property||Value||\n");
            for (key, val) in &props {
                out.push_str(&format!(
                    "|*{}*|{}|\n",
                    capitalize(&escape_wiki(key)),
                    escape_wiki_cell(val),
                ));
            }
            out.push('\n');
        }

        // Attributes sub-section
        let attributes = item.get("attributes").and_then(|v| v.as_array());

        out.push_str("h3. Attributes\n\n");
        match attributes {
            Some(attrs) if !attrs.is_empty() => {
                for attr in attrs {
                    let attr_code = match attr {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    out.push_str(&format!("* {{{{{}}}}}\n", escape_wiki_cell(&attr_code)));
                }
                out.push('\n');
            }
            _ => {
                out.push_str("_No attributes._\n\n");
            }
        }
    }

    out
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
