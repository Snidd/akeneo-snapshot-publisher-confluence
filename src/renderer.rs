use crate::diff::{extract_item_properties, CategoryDiff, DiffReport};
use serde_json::Value;
use std::collections::HashMap;

// =============================================================================
// Diff rendering
// =============================================================================

/// Render a diff page in Confluence storage format (XHTML).
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
    out.push_str(&info_panel(&format!(
        "<strong>Before:</strong> {}<br/><strong>After:</strong> {}",
        escape_html(before),
        escape_html(after),
    )));
    out.push_str("<hr/>");
    out
}

fn render_summary_table(report: &DiffReport) -> String {
    let mut out = String::new();
    out.push_str("<h2>Summary</h2>");

    out.push_str("<table data-layout=\"full-width\"><tbody>");
    out.push_str("<tr><th>Category</th><th>Added</th><th>Removed</th><th>Changed</th></tr>");

    let mut categories: Vec<_> = report.iter().collect();
    categories.sort_by_key(|(name, _)| name.to_lowercase());

    for (name, diff) in &categories {
        out.push_str(&format!(
            "<tr><td><strong>{}</strong></td><td>{}</td><td>{}</td><td>{}</td></tr>",
            capitalize(&escape_html(name)),
            status_badge("Added", diff.added.len(), "Green"),
            status_badge("Removed", diff.removed.len(), "Red"),
            status_badge("Changed", diff.changed.len(), "Yellow"),
        ));
    }

    out.push_str("</tbody></table>");
    out
}

fn render_category(name: &str, diff: &CategoryDiff) -> String {
    let mut out = String::new();
    let display_name = capitalize(&escape_html(name));

    out.push_str(&format!("<h2>{}</h2>", display_name));

    out.push_str(&render_added_section(&diff.added));
    out.push_str(&render_removed_section(&diff.removed));
    out.push_str(&render_changed_section(&diff.changed));

    out
}

fn render_added_section(items: &[Value]) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "<h3>{} Added</h3>",
        status_lozenge(items.len(), "Green"),
    ));

    if items.is_empty() {
        out.push_str("<p><em>No additions.</em></p>");
        return out;
    }

    out.push_str(&render_item_table(items));
    out
}

fn render_removed_section(items: &[Value]) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "<h3>{} Removed</h3>",
        status_lozenge(items.len(), "Red"),
    ));

    if items.is_empty() {
        out.push_str("<p><em>No removals.</em></p>");
        return out;
    }

    out.push_str(&render_item_table(items));
    out
}

fn render_changed_section(items: &[crate::diff::ChangedItem]) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "<h3>{} Changed</h3>",
        status_lozenge(items.len(), "Yellow"),
    ));

    if items.is_empty() {
        out.push_str("<p><em>No changes.</em></p>");
        return out;
    }

    out.push_str("<table data-layout=\"full-width\"><tbody>");
    out.push_str("<tr><th>Code</th><th>Field</th><th>Old Value</th><th>New Value</th></tr>");

    for item in items {
        // Render flat field-level changes (old → new)
        for change in &item.changes {
            out.push_str(&format!(
                "<tr><td><code>{}</code></td><td><code>{}</code></td>\
                 <td><span style=\"color: red;\">{}</span></td>\
                 <td><span style=\"color: green;\">{}</span></td></tr>",
                escape_html(&item.code),
                escape_html(&change.field_path),
                escape_html(&change.old),
                escape_html(&change.new),
            ));
        }

        // Render nested sub-diffs (added/removed within a field)
        for nested in &item.nested_diffs {
            if !nested.added.is_empty() {
                let added_str = nested
                    .added
                    .iter()
                    .map(|v| escape_html(v))
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push_str(&format!(
                    "<tr><td><code>{}</code></td><td><code>{}.added</code></td>\
                     <td></td>\
                     <td><span style=\"color: green;\">{}</span></td></tr>",
                    escape_html(&item.code),
                    escape_html(&nested.field_path),
                    added_str,
                ));
            }
            if !nested.removed.is_empty() {
                let removed_str = nested
                    .removed
                    .iter()
                    .map(|v| escape_html(v))
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push_str(&format!(
                    "<tr><td><code>{}</code></td><td><code>{}.removed</code></td>\
                     <td><span style=\"color: red;\">{}</span></td>\
                     <td></td></tr>",
                    escape_html(&item.code),
                    escape_html(&nested.field_path),
                    removed_str,
                ));
            }
        }
    }

    out.push_str("</tbody></table>");
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

    out.push_str("<table data-layout=\"full-width\"><tbody>");

    // Header row
    out.push_str("<tr>");
    for col in &columns {
        out.push_str(&format!("<th>{}</th>", capitalize(&escape_html(col))));
    }
    out.push_str("</tr>");

    // Data rows
    for props in &all_props {
        let prop_map: HashMap<&str, &str> = props
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        out.push_str("<tr>");
        for col in &columns {
            let val = prop_map.get(col.as_str()).unwrap_or(&"\u{2014}");
            if col == "code" {
                out.push_str(&format!("<td><code>{}</code></td>", escape_html(val)));
            } else {
                out.push_str(&format!("<td>{}</td>", escape_html(val)));
            }
        }
        out.push_str("</tr>");
    }

    out.push_str("</tbody></table>");
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

/// Render a snapshot as a multi-page tree in Confluence storage format (XHTML).
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
            root_body: "<p><em>No data available.</em></p>".to_string(),
            children: Vec::new(),
        };
    };

    // Build root page body: info header + summary table
    let mut root_body = String::new();
    root_body.push_str(&info_panel(&format!(
        "<strong>Snapshot:</strong> {}",
        escape_html(display_label),
    )));
    root_body.push_str("<hr/>");

    root_body.push_str("<h2>Summary</h2>");
    root_body.push_str("<table data-layout=\"full-width\"><tbody>");
    root_body.push_str("<tr><th>Category</th><th>Items</th></tr>");

    let mut categories: Vec<_> = obj.iter().collect();
    categories.sort_by_key(|(name, _)| name.to_lowercase());

    for (name, value) in &categories {
        let count = value.as_array().map(|a| a.len()).unwrap_or(0);
        root_body.push_str(&format!(
            "<tr><td><strong>{}</strong></td><td>{}</td></tr>",
            capitalize(&escape_html(name)),
            count,
        ));
    }
    root_body.push_str("</tbody></table>");

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
        return "<p><em>No items.</em></p>".to_string();
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
        out.push_str("<p><em>No families.</em></p>");
        return out;
    }

    for item in items {
        let code = item
            .get("code")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        // Family heading
        out.push_str(&format!("<h2>{}</h2>", escape_html(code)));

        // Properties table
        let props = extract_item_properties(item);
        if !props.is_empty() {
            out.push_str("<table data-layout=\"full-width\"><tbody>");
            out.push_str("<tr><th>Property</th><th>Value</th></tr>");
            for (key, val) in &props {
                out.push_str(&format!(
                    "<tr><td><strong>{}</strong></td><td>{}</td></tr>",
                    capitalize(&escape_html(key)),
                    escape_html(val),
                ));
            }
            out.push_str("</tbody></table>");
        }

        // Attributes sub-section
        let attributes = item.get("attributes").and_then(|v| v.as_array());

        out.push_str("<h3>Attributes</h3>");
        match attributes {
            Some(attrs) if !attrs.is_empty() => {
                out.push_str("<ul>");
                for attr in attrs {
                    let attr_code = match attr {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    out.push_str(&format!(
                        "<li><code>{}</code></li>",
                        escape_html(&attr_code),
                    ));
                }
                out.push_str("</ul>");
            }
            _ => {
                out.push_str("<p><em>No attributes.</em></p>");
            }
        }
    }

    out
}

// =============================================================================
// Formatting helpers
// =============================================================================

/// Render a Confluence status macro (lozenge badge) in storage format.
fn status_badge(label: &str, count: usize, color: &str) -> String {
    let (title, colour) = if count == 0 {
        (format!("{}: 0", label), "Grey")
    } else {
        (format!("{}: {}", label, count), color)
    };
    format!(
        "<ac:structured-macro ac:name=\"status\">\
         <ac:parameter ac:name=\"title\">{}</ac:parameter>\
         <ac:parameter ac:name=\"colour\">{}</ac:parameter>\
         </ac:structured-macro>",
        escape_html(&title),
        colour,
    )
}

/// Render a Confluence status macro (count-only lozenge) in storage format.
fn status_lozenge(count: usize, color: &str) -> String {
    format!(
        "<ac:structured-macro ac:name=\"status\">\
         <ac:parameter ac:name=\"title\">{}</ac:parameter>\
         <ac:parameter ac:name=\"colour\">{}</ac:parameter>\
         </ac:structured-macro>",
        count, color,
    )
}

/// Render a Confluence info panel in storage format.
fn info_panel(body_html: &str) -> String {
    format!(
        "<ac:structured-macro ac:name=\"info\">\
         <ac:rich-text-body><p>{}</p></ac:rich-text-body>\
         </ac:structured-macro>",
        body_html,
    )
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

/// Escape characters that have special meaning in HTML/XHTML.
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
