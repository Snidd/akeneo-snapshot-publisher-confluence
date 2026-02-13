use crate::diff::{extract_item_properties, CategoryDiff, DiffReport};

/// Render a full Confluence Storage Format (XHTML) page body from a diff report.
pub fn render_page(version: &str, description: &str, report: &DiffReport) -> String {
    let mut html = String::new();

    // Page header with version and description
    html.push_str(&render_header(version, description));

    // Summary panel
    html.push_str(&render_summary_panel(report));

    // Sort categories for deterministic output
    let mut categories: Vec<_> = report.iter().collect();
    categories.sort_by_key(|(name, _)| name.to_lowercase());

    for (category_name, diff) in &categories {
        html.push_str(&render_category(category_name, diff));
    }

    html
}

/// Generate the page title for Confluence.
pub fn page_title(version: &str) -> String {
    format!("Release Notes — v{}", version)
}

fn render_header(version: &str, description: &str) -> String {
    let mut html = String::new();

    // Info panel with version and description
    html.push_str(r#"<ac:structured-macro ac:name="info"><ac:rich-text-body>"#);
    html.push_str(&format!(
        "<p><strong>Version:</strong> {}</p>",
        escape_html(version)
    ));
    html.push_str(&format!(
        "<p><strong>Description:</strong> {}</p>",
        escape_html(description)
    ));
    html.push_str("</ac:rich-text-body></ac:structured-macro>");

    html.push_str("<hr />");

    html
}

fn render_summary_panel(report: &DiffReport) -> String {
    let mut html = String::new();

    html.push_str("<h2>Summary</h2>");
    html.push_str(r#"<table><colgroup><col /><col /><col /><col /></colgroup>"#);
    html.push_str("<thead><tr>");
    html.push_str("<th>Category</th>");
    html.push_str("<th>Added</th>");
    html.push_str("<th>Removed</th>");
    html.push_str("<th>Changed</th>");
    html.push_str("</tr></thead>");
    html.push_str("<tbody>");

    let mut categories: Vec<_> = report.iter().collect();
    categories.sort_by_key(|(name, _)| name.to_lowercase());

    for (name, diff) in &categories {
        html.push_str("<tr>");
        html.push_str(&format!(
            "<td><strong>{}</strong></td>",
            capitalize(&escape_html(name))
        ));
        html.push_str(&format!(
            "<td>{}</td>",
            status_badge("Added", diff.added.len(), "Green")
        ));
        html.push_str(&format!(
            "<td>{}</td>",
            status_badge("Removed", diff.removed.len(), "Red")
        ));
        html.push_str(&format!(
            "<td>{}</td>",
            status_badge("Changed", diff.changed.len(), "Yellow")
        ));
        html.push_str("</tr>");
    }

    html.push_str("</tbody></table>");
    html
}

fn render_category(name: &str, diff: &CategoryDiff) -> String {
    let mut html = String::new();
    let display_name = capitalize(&escape_html(name));

    html.push_str(&format!("<h2>{}</h2>", display_name));

    // Added section
    html.push_str(&render_added_section(&diff.added));

    // Removed section
    html.push_str(&render_removed_section(&diff.removed));

    // Changed section
    html.push_str(&render_changed_section(&diff.changed));

    html
}

fn render_added_section(items: &[serde_json::Value]) -> String {
    let mut html = String::new();

    html.push_str(&format!(
        "<h3>{} Added</h3>",
        status_lozenge(items.len(), "Green")
    ));

    if items.is_empty() {
        html.push_str("<p><em>No additions.</em></p>");
        return html;
    }

    html.push_str(&render_item_table(items));
    html
}

fn render_removed_section(items: &[serde_json::Value]) -> String {
    let mut html = String::new();

    html.push_str(&format!(
        "<h3>{} Removed</h3>",
        status_lozenge(items.len(), "Red")
    ));

    if items.is_empty() {
        html.push_str("<p><em>No removals.</em></p>");
        return html;
    }

    html.push_str(&render_item_table(items));
    html
}

fn render_changed_section(items: &[crate::diff::ChangedItem]) -> String {
    let mut html = String::new();

    html.push_str(&format!(
        "<h3>{} Changed</h3>",
        status_lozenge(items.len(), "Yellow")
    ));

    if items.is_empty() {
        html.push_str("<p><em>No changes.</em></p>");
        return html;
    }

    html.push_str(r#"<table><colgroup><col /><col /><col /><col /></colgroup>"#);
    html.push_str("<thead><tr>");
    html.push_str("<th>Code</th>");
    html.push_str("<th>Field</th>");
    html.push_str("<th>Old Value</th>");
    html.push_str("<th>New Value</th>");
    html.push_str("</tr></thead>");
    html.push_str("<tbody>");

    for item in items {
        let row_count = item.changes.len();
        for (i, change) in item.changes.iter().enumerate() {
            html.push_str("<tr>");
            if i == 0 {
                if row_count > 1 {
                    html.push_str(&format!(
                        r#"<td rowspan="{}">{}</td>"#,
                        row_count,
                        code_markup(&item.code)
                    ));
                } else {
                    html.push_str(&format!("<td>{}</td>", code_markup(&item.code)));
                }
            }
            html.push_str(&format!(
                "<td><code>{}</code></td>",
                escape_html(&change.field_path)
            ));
            html.push_str(&format!("<td>{}</td>", old_value_markup(&change.old)));
            html.push_str(&format!("<td>{}</td>", new_value_markup(&change.new)));
            html.push_str("</tr>");
        }
    }

    html.push_str("</tbody></table>");
    html
}

/// Render a table of added/removed items using their extracted properties.
fn render_item_table(items: &[serde_json::Value]) -> String {
    // Collect all properties from all items to determine columns
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

    let mut html = String::new();
    html.push_str("<table><colgroup>");
    for _ in &columns {
        html.push_str("<col />");
    }
    html.push_str("</colgroup>");

    // Header row
    html.push_str("<thead><tr>");
    for col in &columns {
        html.push_str(&format!("<th>{}</th>", capitalize(&escape_html(col))));
    }
    html.push_str("</tr></thead>");

    // Data rows
    html.push_str("<tbody>");
    for props in &all_props {
        html.push_str("<tr>");
        let prop_map: std::collections::HashMap<&str, &str> = props
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        for col in &columns {
            let val = prop_map.get(col.as_str()).unwrap_or(&"—");
            if col == "code" {
                html.push_str(&format!("<td>{}</td>", code_markup(val)));
            } else {
                html.push_str(&format!("<td>{}</td>", escape_html(val)));
            }
        }
        html.push_str("</tr>");
    }
    html.push_str("</tbody></table>");

    html
}

// --- Formatting helpers ---

fn status_badge(label: &str, count: usize, color: &str) -> String {
    if count == 0 {
        return format!(
            r#"<ac:structured-macro ac:name="status"><ac:parameter ac:name="title">{}: 0</ac:parameter><ac:parameter ac:name="colour">Grey</ac:parameter></ac:structured-macro>"#,
            label
        );
    }
    format!(
        r#"<ac:structured-macro ac:name="status"><ac:parameter ac:name="title">{}: {}</ac:parameter><ac:parameter ac:name="colour">{}</ac:parameter></ac:structured-macro>"#,
        label, count, color
    )
}

fn status_lozenge(count: usize, color: &str) -> String {
    format!(
        r#"<ac:structured-macro ac:name="status"><ac:parameter ac:name="title">{}</ac:parameter><ac:parameter ac:name="colour">{}</ac:parameter></ac:structured-macro>"#,
        count, color
    )
}

fn code_markup(text: &str) -> String {
    format!("<code>{}</code>", escape_html(text))
}

fn old_value_markup(text: &str) -> String {
    format!(
        r#"<span style="color: #de350b;">{}</span>"#,
        escape_html(text)
    )
}

fn new_value_markup(text: &str) -> String {
    format!(
        r#"<span style="color: #36b37e;">{}</span>"#,
        escape_html(text)
    )
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
