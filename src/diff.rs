use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Represents the entire diff file: a map of category names (e.g. "attributes", "families")
/// to their respective diffs.
pub type DiffReport = HashMap<String, CategoryDiff>;

/// A diff for a single category, containing added, removed, and changed items.
#[derive(Debug)]
pub struct CategoryDiff {
    pub added: Vec<Value>,
    pub removed: Vec<Value>,
    pub changed: Vec<ChangedItem>,
}

/// An item that was changed, identified by its code, with a set of field-level changes.
#[derive(Debug)]
pub struct ChangedItem {
    pub code: String,
    pub changes: Vec<FieldChange>,
}

/// A single field-level change, with a dotted path (e.g. "labels.en_US"), old value, and new value.
#[derive(Debug)]
pub struct FieldChange {
    pub field_path: String,
    pub old: String,
    pub new: String,
}

/// Parse a diff JSON file into a `DiffReport`.
pub fn parse_diff_file(path: &Path) -> Result<DiffReport> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read diff file: {}", path.display()))?;
    let root: Value =
        serde_json::from_str(&content).with_context(|| "Failed to parse diff JSON")?;

    let obj = root
        .as_object()
        .with_context(|| "Diff JSON root must be an object")?;

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
    for (field_name, field_value) in changes_obj {
        flatten_changes(field_name, field_value, &mut changes);
    }

    Some(ChangedItem { code, changes })
}

/// Recursively flatten nested change objects into a flat list of `FieldChange`.
///
/// A leaf change has `{"old": ..., "new": ...}`.
/// A nested change has sub-keys that themselves contain changes,
/// e.g. `{"labels": {"en_US": {"old": "...", "new": "..."}}}`.
fn flatten_changes(prefix: &str, value: &Value, out: &mut Vec<FieldChange>) {
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

    // Otherwise recurse into sub-keys
    for (key, sub_value) in obj {
        let path = format!("{}.{}", prefix, key);
        flatten_changes(&path, sub_value, out);
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
        if let Some(val) = obj.get(field) {
            if !val.is_null() {
                props.push((field.to_string(), format_value(val)));
            }
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
