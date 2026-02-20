use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;

/// Represents the entire diff: a map of category names (e.g. "attributes", "families")
/// to their respective diffs.
pub type DiffReport = HashMap<String, CategoryDiff>;

/// A diff for a single category, containing added, removed, and changed items.
#[derive(Debug)]
pub struct CategoryDiff {
    pub added: Vec<Value>,
    pub removed: Vec<Value>,
    pub changed: Vec<ChangedItem>,
}

/// An item that was changed, identified by its code, with a set of field-level changes
/// and optional nested sub-diffs (e.g. added/removed items within a field).
#[derive(Debug)]
pub struct ChangedItem {
    pub code: String,
    pub changes: Vec<FieldChange>,
    pub nested_diffs: Vec<NestedFieldDiff>,
}

/// A single field-level change, with a dotted path (e.g. "labels.en_US"), old value, and new value.
#[derive(Debug)]
pub struct FieldChange {
    pub field_path: String,
    pub old: String,
    pub new: String,
}

/// A nested sub-diff within a changed item's field, containing added/removed lists.
/// For example, a family's "attributes" field may have added or removed attribute codes.
#[derive(Debug)]
pub struct NestedFieldDiff {
    pub field_path: String,
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

/// Parse diff data from a JSON value (typically the `data` JSONB column from the database).
pub fn parse_diff_data(root: &Value) -> Result<DiffReport> {
    let obj = root
        .as_object()
        .context("Diff data root must be an object")?;

    let mut report = DiffReport::new();

    for (category_name, category_value) in obj {
        let cat_obj = category_value
            .as_object()
            .with_context(|| format!("Category '{}' must be an object", category_name))?;

        let added = cat_obj
            .get("added")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let removed = cat_obj
            .get("removed")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let changed_raw = cat_obj
            .get("changed")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let changed = changed_raw
            .into_iter()
            .filter_map(|item| parse_changed_item(&item))
            .collect();

        report.insert(
            category_name.clone(),
            CategoryDiff {
                added,
                removed,
                changed,
            },
        );
    }

    Ok(report)
}

/// Parse a single changed item from the JSON value.
fn parse_changed_item(value: &Value) -> Option<ChangedItem> {
    let obj = value.as_object()?;
    let code = obj.get("code")?.as_str()?.to_string();
    let changes_value = obj.get("changes")?;
    let changes_obj = changes_value.as_object()?;

    let mut changes = Vec::new();
    let mut nested_diffs = Vec::new();
    for (field_name, field_value) in changes_obj {
        flatten_changes(field_name, field_value, &mut changes, &mut nested_diffs);
    }

    Some(ChangedItem {
        code,
        changes,
        nested_diffs,
    })
}

/// Recursively flatten nested change objects into a flat list of `FieldChange`,
/// and collect any nested sub-diffs (added/removed arrays) into `NestedFieldDiff`.
///
/// A leaf change has `{"old": ..., "new": ...}`.
/// A nested sub-diff has `{"added": [...], "removed": [...]}`.
/// A nested change has sub-keys that themselves contain changes,
/// e.g. `{"labels": {"en_US": {"old": "...", "new": "..."}}}`.
fn flatten_changes(
    prefix: &str,
    value: &Value,
    out: &mut Vec<FieldChange>,
    nested_out: &mut Vec<NestedFieldDiff>,
) {
    let Some(obj) = value.as_object() else {
        return;
    };

    // Check if this is a leaf: has both "old" and "new" keys
    if obj.contains_key("old") && obj.contains_key("new") {
        let old = format_value(&obj["old"]);
        let new = format_value(&obj["new"]);
        out.push(FieldChange {
            field_path: prefix.to_string(),
            old,
            new,
        });
        return;
    }

    // Check if this is a nested sub-diff: has "added" and/or "removed" keys (arrays)
    let has_added = obj.get("added").is_some_and(|v| v.is_array());
    let has_removed = obj.get("removed").is_some_and(|v| v.is_array());

    if has_added || has_removed {
        let added = obj
            .get("added")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().map(format_value).collect())
            .unwrap_or_default();

        let removed = obj
            .get("removed")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().map(format_value).collect())
            .unwrap_or_default();

        nested_out.push(NestedFieldDiff {
            field_path: prefix.to_string(),
            added,
            removed,
        });
        return;
    }

    // Otherwise recurse into sub-keys
    for (key, sub_value) in obj {
        let path = format!("{}.{}", prefix, key);
        flatten_changes(&path, sub_value, out, nested_out);
    }
}

/// Format a JSON value as a human-readable string for display.
fn format_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

/// Extract a human-readable summary of key properties from an added/removed item.
/// Returns a list of (key, value) pairs for display in a table.
pub fn extract_item_properties(item: &Value) -> Vec<(String, String)> {
    let Some(obj) = item.as_object() else {
        return vec![("value".to_string(), item.to_string())];
    };

    // Priority fields to show first (in order)
    let priority_fields = ["code", "type", "group"];
    let mut props = Vec::new();

    for &field in &priority_fields {
        if let Some(val) = obj.get(field) && !val.is_null() {
            props.push((field.to_string(), format_value(val)));
        }
    }

    // Extract labels (flatten the labels object)
    if let Some(labels) = obj.get("labels").and_then(|v| v.as_object()) {
        for (locale, label_val) in labels {
            props.push((format!("label ({})", locale), format_value(label_val)));
        }
    }

    // Add other notable non-null, non-default fields
    let skip_fields = [
        "code",
        "type",
        "group",
        "labels",
        "group_labels",
        "attributes",
        "decimal_places",
        "default_value",
        "display_time",
        "is_read_only",
        "max_characters",
        "max_file_size",
        "max_items_count",
        "minimum_input_length",
        "number_max",
        "number_min",
        "reference_data_name",
        "validation_rule",
    ];

    for (key, val) in obj {
        if skip_fields.contains(&key.as_str()) {
            continue;
        }
        if val.is_null() {
            continue;
        }
        // Skip false booleans and empty values to reduce noise
        if val.as_bool() == Some(false) {
            continue;
        }
        if val.as_array().is_some_and(|a| a.is_empty()) {
            continue;
        }
        if val.as_object().is_some_and(|o| o.is_empty()) {
            continue;
        }
        props.push((key.clone(), format_value(val)));
    }

    props
}
