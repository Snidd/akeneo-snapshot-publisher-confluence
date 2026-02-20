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
        items.iter().map(extract_item_properties).collect();

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
/// The root page contains the full overview (summary cards + all category tables),
/// and each family gets its own child page with detailed configuration.
pub struct SnapshotPageTree {
    pub root_title: String,
    pub root_body: String,
    pub children: Vec<SnapshotChildPage>,
}

/// A single child page (one per family in the snapshot).
pub struct SnapshotChildPage {
    pub title: String,
    pub body: String,
}

/// Render a snapshot as a multi-page tree in Confluence storage format (XHTML).
///
/// Returns a `SnapshotPageTree` with:
/// - A root "Akeneo Model Snapshot" page containing summary cards and all category tables
/// - One child page per family with detailed configuration, attribute requirements, and
///   enriched attribute tables cross-referenced against the snapshot's attribute data
pub fn render_snapshot_pages(label: Option<&str>, data: &Value) -> SnapshotPageTree {
    let _display_label = label.unwrap_or("Unnamed snapshot");
    let root_title = "Current model".to_string();

    let Some(obj) = data.as_object() else {
        return SnapshotPageTree {
            root_title,
            root_body: "<p><em>No data available.</em></p>".to_string(),
            children: Vec::new(),
        };
    };

    let channels = obj
        .get("channels")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let families = obj
        .get("families")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let attributes = obj
        .get("attributes")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let categories = obj
        .get("categories")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let attribute_options = obj.get("attribute_options");

    // Count attribute options (it's a dict of attribute_code -> [options])
    let attr_options_count: usize = attribute_options
        .and_then(|v| v.as_object())
        .map(|o| {
            o.values()
                .filter_map(|v| v.as_array())
                .map(|a| a.len())
                .sum()
        })
        .unwrap_or(0);

    // ── Root page body ──────────────────────────────────────────────────
    let mut body = String::new();

    // Title section
    body.push_str("<h1>Akeneo Model Snapshot</h1>");
    body.push_str("<p>Overview of the PIM data model configuration \u{2014} channels, families, attributes, categories, and attribute options.</p>");
    body.push_str("<hr/>");

    // Summary cards (rendered as a table)
    body.push_str(&render_summary_cards(
        channels.len(),
        families.len(),
        attributes.len(),
        categories.len(),
        attr_options_count,
    ));

    // Category sections
    body.push_str(&render_channels_section(&channels));
    body.push_str(&render_families_section(&families));
    body.push_str(&render_attributes_section(&attributes));
    body.push_str(&render_categories_section(&categories));
    body.push_str(&render_attribute_options_sections(attribute_options));

    // ── Child pages (one per family) ────────────────────────────────────
    let children: Vec<SnapshotChildPage> = families
        .iter()
        .map(|family| {
            let code = family
                .get("code")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let label = get_label(family).unwrap_or_else(|| code.to_string());
            let page_title = format!("Family: {} ({})", label, code);
            let page_body = render_family_detail_page(family, &attributes);
            SnapshotChildPage {
                title: page_title,
                body: page_body,
            }
        })
        .collect();

    SnapshotPageTree {
        root_title,
        root_body: body,
        children,
    }
}

// =============================================================================
// Overview page sections
// =============================================================================

/// Render the summary cards as a 5-column table with large counts and labels.
fn render_summary_cards(
    channels: usize,
    families: usize,
    attributes: usize,
    categories: usize,
    attr_options: usize,
) -> String {
    let mut out = String::new();
    out.push_str("<table data-layout=\"full-width\"><tbody><tr>");

    let cards = [
        ("\u{1F4E1}", channels, "Channels"),
        ("\u{1F4DA}", families, "Families"),
        ("\u{2699}\u{FE0F}", attributes, "Attributes"),
        ("\u{1F4C2}", categories, "Categories"),
        ("\u{1F4CB}", attr_options, "Attr. Options"),
    ];

    for (icon, count, label) in &cards {
        out.push_str(&format!(
            "<td><p>{}</p><p><strong style=\"font-size: 24px;\">{}</strong></p><p><em>{}</em></p></td>",
            icon, count, label,
        ));
    }

    out.push_str("</tr></tbody></table>");
    out
}

/// Render the Channels section with a structured table.
fn render_channels_section(channels: &[Value]) -> String {
    let mut out = String::new();
    out.push_str(&section_heading("Channels", channels.len(), "Green"));

    if channels.is_empty() {
        out.push_str("<p><em>No channels.</em></p>");
        return out;
    }

    out.push_str("<table data-layout=\"full-width\"><tbody>");
    out.push_str("<tr><th>Code</th><th>Label</th><th>Locales</th><th>Currencies</th><th>Category Tree</th></tr>");

    for ch in channels {
        let code = get_code(ch);
        let label = get_label(ch).unwrap_or_else(|| "\u{2014}".to_string());
        let locales = get_string_array(ch, "locales").join(", ");
        let currencies = get_string_array(ch, "currencies").join(", ");
        let tree = ch
            .get("category_tree")
            .and_then(|v| v.as_str())
            .unwrap_or("\u{2014}");

        out.push_str(&format!(
            "<tr><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape_html(code),
            escape_html(&label),
            escape_html(&locales),
            escape_html(&currencies),
            escape_html(tree),
        ));
    }

    out.push_str("</tbody></table>");
    out
}

/// Render the Families section with a structured table.
fn render_families_section(families: &[Value]) -> String {
    let mut out = String::new();
    out.push_str(&section_heading("Families", families.len(), "Yellow"));

    if families.is_empty() {
        out.push_str("<p><em>No families.</em></p>");
        return out;
    }

    out.push_str("<table data-layout=\"full-width\"><tbody>");
    out.push_str("<tr><th>Code</th><th>Label</th><th>Attributes</th><th>Label Attr</th><th>Image Attr</th></tr>");

    for fam in families {
        let code = get_code(fam);
        let label = get_label(fam).unwrap_or_else(|| "\u{2014}".to_string());
        let attr_count = fam
            .get("attributes")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        let label_attr = fam
            .get("attribute_as_label")
            .and_then(|v| v.as_str())
            .unwrap_or("\u{2014}");
        let image_attr = fam
            .get("attribute_as_image")
            .and_then(|v| v.as_str())
            .unwrap_or("\u{2014}");

        out.push_str(&format!(
            "<tr><td><code>{}</code></td><td>{}</td><td>{}</td><td><code>{}</code></td><td><code>{}</code></td></tr>",
            escape_html(code),
            escape_html(&label),
            status_lozenge(attr_count, "Blue"),
            escape_html(label_attr),
            escape_html(image_attr),
        ));
    }

    out.push_str("</tbody></table>");
    out
}

/// Render the Attributes section with a structured table.
fn render_attributes_section(attributes: &[Value]) -> String {
    let mut out = String::new();
    out.push_str(&section_heading("Attributes", attributes.len(), "Purple"));

    if attributes.is_empty() {
        out.push_str("<p><em>No attributes.</em></p>");
        return out;
    }

    out.push_str("<table data-layout=\"full-width\"><tbody>");
    out.push_str("<tr><th>Code</th><th>Label</th><th>Type</th><th>Group</th><th>Scopable</th><th>Localizable</th></tr>");

    for attr in attributes {
        let code = get_code(attr);
        let label = get_label(attr).unwrap_or_else(|| "\u{2014}".to_string());
        let attr_type = attr
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("\u{2014}");
        let group = attr
            .get("group")
            .and_then(|v| v.as_str())
            .unwrap_or("\u{2014}");
        let scopable = attr
            .get("scopable")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let localizable = attr
            .get("localizable")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        out.push_str(&format!(
            "<tr><td><code>{}</code></td><td>{}</td><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape_html(code),
            escape_html(&label),
            escape_html(attr_type),
            escape_html(group),
            check_icon(scopable),
            check_icon(localizable),
        ));
    }

    out.push_str("</tbody></table>");
    out
}

/// Render the Categories section with a structured table.
fn render_categories_section(categories: &[Value]) -> String {
    let mut out = String::new();
    out.push_str(&section_heading("Categories", categories.len(), "Blue"));

    if categories.is_empty() {
        out.push_str("<p><em>No categories.</em></p>");
        return out;
    }

    out.push_str("<table data-layout=\"full-width\"><tbody>");
    out.push_str("<tr><th>Code</th><th>Labels</th><th>Parent</th><th>Updated</th></tr>");

    for cat in categories {
        let code = get_code(cat);
        let labels = render_labels_inline(cat);
        let parent = cat
            .get("parent")
            .and_then(|v| v.as_str())
            .unwrap_or("\u{2014}");
        let updated = cat
            .get("updated")
            .and_then(|v| v.as_str())
            .unwrap_or("\u{2014}");

        out.push_str(&format!(
            "<tr><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape_html(code),
            labels,
            escape_html(parent),
            escape_html(updated),
        ));
    }

    out.push_str("</tbody></table>");
    out
}

/// Render the Attribute Options section, grouped by parent attribute.
/// The `options_value` is expected to be a JSON object mapping attribute codes
/// to arrays of option objects.
fn render_attribute_options_sections(options_value: Option<&Value>) -> String {
    let mut out = String::new();

    let Some(obj) = options_value.and_then(|v| v.as_object()) else {
        out.push_str(&section_heading("Attribute Options", 0, "Grey"));
        out.push_str("<p><em>No attribute options.</em></p>");
        return out;
    };

    let total: usize = obj
        .values()
        .filter_map(|v| v.as_array())
        .map(|a| a.len())
        .sum();

    out.push_str(&section_heading("Attribute Options", total, "Yellow"));

    let mut attr_codes: Vec<&String> = obj.keys().collect();
    attr_codes.sort();

    for attr_code in attr_codes {
        let options = match obj.get(attr_code).and_then(|v| v.as_array()) {
            Some(arr) => arr,
            None => continue,
        };

        out.push_str(&format!(
            "<h3>Attribute: <code>{}</code> {}</h3>",
            escape_html(attr_code),
            status_lozenge(options.len(), "Grey"),
        ));

        if options.is_empty() {
            out.push_str("<p><em>No options.</em></p>");
            continue;
        }

        out.push_str("<table data-layout=\"full-width\"><tbody>");
        out.push_str("<tr><th>Code</th><th>Label</th><th>Sort Order</th></tr>");

        for opt in options {
            let code = get_code(opt);
            let label = get_label(opt).unwrap_or_else(|| "\u{2014}".to_string());
            let sort_order = opt
                .get("sort_order")
                .map(|v| match v {
                    Value::Number(n) => n.to_string(),
                    _ => v.to_string(),
                })
                .unwrap_or_else(|| "\u{2014}".to_string());

            out.push_str(&format!(
                "<tr><td><code>{}</code></td><td>{}</td><td>{}</td></tr>",
                escape_html(code),
                escape_html(&label),
                escape_html(&sort_order),
            ));
        }

        out.push_str("</tbody></table>");
    }

    out
}

// =============================================================================
// Family detail child pages
// =============================================================================

/// Render a detailed family page with configuration metadata, attribute requirements,
/// and an enriched attributes table cross-referenced against the snapshot's attribute data.
fn render_family_detail_page(family: &Value, all_attributes: &[Value]) -> String {
    let mut out = String::new();

    let code = get_code(family);
    let label = get_label(family).unwrap_or_else(|| code.to_string());

    // Build an attribute lookup map for cross-referencing
    let attr_map: HashMap<&str, &Value> = all_attributes
        .iter()
        .filter_map(|a| a.get("code").and_then(|c| c.as_str()).map(|c| (c, a)))
        .collect();

    // ── Title ────────────────────────────────────────────────────────────
    out.push_str(&format!("<h1>{}</h1>", escape_html(&label),));
    out.push_str(&format!(
        "<p><code>{}</code> \u{2014} Family configuration and associated attributes from the Akeneo PIM snapshot.</p>",
        escape_html(code),
    ));
    out.push_str("<hr/>");

    // ── Family Configuration ────────────────────────────────────────────
    out.push_str("<h2>Family Configuration</h2>");

    let parent = family
        .get("parent")
        .and_then(|v| v.as_str())
        .unwrap_or("\u{2014} No parent");
    let label_attr = family
        .get("attribute_as_label")
        .and_then(|v| v.as_str())
        .unwrap_or("\u{2014}");
    let image_attr = family
        .get("attribute_as_image")
        .and_then(|v| v.as_str())
        .unwrap_or("\u{2014}");
    let family_attrs = family.get("attributes").and_then(|v| v.as_array());
    let total_attrs = family_attrs.map(|a| a.len()).unwrap_or(0);

    // Render as a 3-column x 2-row metadata table
    out.push_str("<table data-layout=\"full-width\"><tbody>");
    out.push_str("<tr>");
    out.push_str(&format!(
        "<td><strong>Family Code</strong><br/><code>{}</code></td>",
        escape_html(code),
    ));
    out.push_str(&format!(
        "<td><strong>Label</strong><br/>{}</td>",
        escape_html(&label),
    ));
    out.push_str(&format!(
        "<td><strong>Parent</strong><br/>{}</td>",
        escape_html(parent),
    ));
    out.push_str("</tr><tr>");
    out.push_str(&format!(
        "<td><strong>Attribute as Label</strong><br/><code>{}</code></td>",
        escape_html(label_attr),
    ));
    out.push_str(&format!(
        "<td><strong>Attribute as Image</strong><br/><code>{}</code></td>",
        escape_html(image_attr),
    ));
    out.push_str(&format!(
        "<td><strong>Total Attributes</strong><br/><strong style=\"font-size: 24px;\">{}</strong></td>",
        total_attrs,
    ));
    out.push_str("</tr></tbody></table>");

    // ── Attribute Requirements ───────────────────────────────────────────
    out.push_str("<h2>Attribute Requirements</h2>");

    let requirements = family
        .get("attribute_requirements")
        .and_then(|v| v.as_object());

    match requirements {
        Some(reqs) if !reqs.is_empty() => {
            out.push_str("<table data-layout=\"full-width\"><tbody>");
            out.push_str("<tr><th>Channel</th><th>Required Attributes</th></tr>");

            let mut channels: Vec<_> = reqs.iter().collect();
            channels.sort_by_key(|(name, _)| name.to_lowercase());

            for (channel, attrs_val) in channels {
                let attrs = attrs_val
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str())
                            .map(|s| format!("<code>{}</code>", escape_html(s)))
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_else(|| "\u{2014}".to_string());

                out.push_str(&format!(
                    "<tr><td><strong>{}</strong></td><td>{}</td></tr>",
                    escape_html(channel),
                    attrs,
                ));
            }

            out.push_str("</tbody></table>");
        }
        _ => {
            out.push_str("<p><em>No attribute requirements defined.</em></p>");
        }
    }

    // ── Family Attributes (enriched) ────────────────────────────────────
    out.push_str(&format!(
        "<h2>Family Attributes {}</h2>",
        status_lozenge(total_attrs, "Purple"),
    ));

    match family_attrs {
        Some(attrs) if !attrs.is_empty() => {
            // Build a set of required attributes per channel for this family
            let required_map: HashMap<&str, Vec<&str>> = requirements
                .map(|reqs| {
                    reqs.iter()
                        .filter_map(|(ch, arr)| {
                            arr.as_array().map(|a| {
                                (
                                    ch.as_str(),
                                    a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>(),
                                )
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

            out.push_str("<table data-layout=\"full-width\"><tbody>");
            out.push_str("<tr><th>Attribute Code</th><th>Type</th><th>Group</th><th>Scopable</th><th>Localizable</th><th>Required</th></tr>");

            for attr_val in attrs {
                let attr_code = attr_val.as_str().unwrap_or("unknown");

                // Cross-reference with the snapshot's attributes data
                let (attr_type, group, scopable, localizable) =
                    if let Some(attr_data) = attr_map.get(attr_code) {
                        (
                            attr_data
                                .get("type")
                                .and_then(|v| v.as_str())
                                .unwrap_or("\u{2014}"),
                            attr_data
                                .get("group")
                                .and_then(|v| v.as_str())
                                .unwrap_or("\u{2014}"),
                            attr_data
                                .get("scopable")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false),
                            attr_data
                                .get("localizable")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false),
                        )
                    } else {
                        ("\u{2014}", "\u{2014}", false, false)
                    };

                // Determine which channels require this attribute
                let required_channels: Vec<&str> = required_map
                    .iter()
                    .filter(|(_, req_attrs)| req_attrs.contains(&attr_code))
                    .map(|(ch, _)| *ch)
                    .collect();

                let required_display = if required_channels.is_empty() {
                    "\u{2014}".to_string()
                } else {
                    required_channels
                        .iter()
                        .map(|ch| escape_html(ch))
                        .collect::<Vec<_>>()
                        .join(", ")
                };

                out.push_str(&format!(
                    "<tr><td><code>{}</code></td><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                    escape_html(attr_code),
                    escape_html(attr_type),
                    escape_html(group),
                    check_icon(scopable),
                    check_icon(localizable),
                    required_display,
                ));
            }

            out.push_str("</tbody></table>");
        }
        _ => {
            out.push_str("<p><em>No attributes in this family.</em></p>");
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

// =============================================================================
// Snapshot-specific helpers
// =============================================================================

/// Render a section heading with an uppercase label and a count lozenge.
fn section_heading(label: &str, count: usize, color: &str) -> String {
    format!(
        "<h2>{} {}</h2>",
        escape_html(&label.to_uppercase()),
        status_lozenge(count, color),
    )
}

/// Render a checkmark or X icon for boolean values.
fn check_icon(val: bool) -> &'static str {
    if val {
        "\u{2705}" // green checkmark emoji
    } else {
        "\u{274C}" // red X emoji
    }
}

/// Extract the "code" field from a JSON object.
fn get_code(item: &Value) -> &str {
    item.get("code")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
}

/// Extract the first available label from a JSON object's "labels" field.
fn get_label(item: &Value) -> Option<String> {
    item.get("labels")
        .and_then(|v| v.as_object())
        .and_then(|labels| labels.values().next())
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Extract an array of strings from a JSON object field.
fn get_string_array(item: &Value, field: &str) -> Vec<String> {
    item.get(field)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Render labels as inline locale-tagged text (e.g., "en_GB: Label, de_AT: Label").
fn render_labels_inline(item: &Value) -> String {
    item.get("labels")
        .and_then(|v| v.as_object())
        .map(|labels| {
            labels
                .iter()
                .map(|(locale, val)| {
                    let text = val.as_str().unwrap_or("\u{2014}");
                    format!(
                        "<strong>{}</strong>: {}",
                        escape_html(locale),
                        escape_html(text),
                    )
                })
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_else(|| "\u{2014}".to_string())
}
