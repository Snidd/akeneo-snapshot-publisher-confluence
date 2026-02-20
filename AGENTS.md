# AGENTS.md — Confluence Documenter

## Project Overview

A Rust web service that reads Akeneo PIM snapshot and diff data from PostgreSQL, renders it as Confluence Storage Format (XHTML) pages, and publishes them to Confluence Cloud via REST API.

**Tech stack:** Rust 2024 edition, Axum 0.8 (HTTP), sqlx 0.8 (Postgres), reqwest 0.12 (HTTP client), serde_json (all data is untyped `serde_json::Value`).

**No template engine.** All Confluence XHTML is built via procedural string concatenation in `renderer.rs`.

---

## Architecture & Data Flow

```
PostgreSQL                   Rust Service                    Confluence Cloud
┌──────────────┐    ┌─────────────────────────────┐    ┌──────────────────┐
│ snapshot      │    │  main.rs                    │    │  REST API v1     │
│ diff          │───>│    ├─ db.rs (fetch)         │    │  /wiki/rest/api/ │
│ confluence_   │    │    ├─ diff.rs (parse)        │───>│    content       │
│   config      │    │    ├─ renderer.rs (render)   │    │                  │
│ akeneo_server │    │    └─ confluence.rs (publish) │    │  Storage format  │
└──────────────┘    └─────────────────────────────┘    └──────────────────┘
```

**Snapshot path:** `db::fetch_snapshot` -> `renderer::render_snapshot_pages` -> `confluence::publish_page` (root) + `publish_page_under_id` (children)

**Diff path:** `db::fetch_diff` -> `diff::parse_diff_data` -> `renderer::render_diff_page` -> `confluence::publish_page`

---

## Source File Guide

### `src/main.rs` (~300 lines)
HTTP server setup with Axum. Two GET endpoints:
- `GET /api/snapshot/{id}` — Fetch snapshot by UUID, render multi-page tree, publish all pages, return root URL.
- `GET /api/diff/{id}` — Fetch diff + both snapshots, parse diff, render single page, publish, return URL.

Key types: `AppState { pool: PgPool }`, `SuccessResponse`, `ErrorResponse`.

### `src/db.rs` (~116 lines)
PostgreSQL queries using sqlx with raw SQL.
- `connect()` — Creates PgPool from `DATABASE_URL` env var.
- `fetch_snapshot(pool, id)` -> `SnapshotRow { id, akeneo_server_id, label, started_at, completed_at, data: Value }`
- `fetch_diff(pool, id)` -> `(DiffRow, SnapshotRow, SnapshotRow)` — uses `tokio::try_join!` for parallel fetch.
- `fetch_confluence_config(pool, akeneo_server_id)` -> `DbConfluenceConfig { base_url, username, api_token, space_key, parent_page }`

### `src/diff.rs` (~253 lines)
Parses raw diff JSON into structured Rust types.
- `DiffReport` = `HashMap<String, CategoryDiff>`
- `CategoryDiff { added: Vec<Value>, removed: Vec<Value>, changed: Vec<ChangedItem> }`
- `ChangedItem { code, changes: Vec<FieldChange>, nested_diffs: Vec<NestedFieldDiff> }`
- `FieldChange { field_path, old, new }` — dotted paths like "labels.en_US"
- `NestedFieldDiff { field_path, added: Vec<String>, removed: Vec<String> }`
- `extract_item_properties(item: &Value)` — Extracts display-ready key/value pairs with priority ordering ("code", "type", "group" first), label flattening, and noise reduction. Used by diff rendering.
- `flatten_changes()` — Recursive flattener that detects leaf changes (`{old, new}`), nested sub-diffs (`{added, removed}`), and nested objects.

### `src/renderer.rs` (~964 lines)
The core rendering engine. Two independent sections:

**Diff rendering (lines 1-239):** Unchanged from original design.
- `render_diff_page(before_label, after_label, report)` -> `(title, body)` — Single page with summary table + per-category sections.
- Uses `render_item_table()` for added/removed items (generic, auto-detecting columns from `extract_item_properties`).
- Changed items rendered as Code | Field | Old Value (red) | New Value (green) tables.

**Snapshot rendering (lines 241-835):** Redesigned to match UI design (see "UI Design Reference" section below).
- `render_snapshot_pages(label, data)` -> `SnapshotPageTree { root_title, root_body, children: Vec<SnapshotChildPage> }`
- **Root page** ("Current model") contains:
  1. Title "Akeneo Model Snapshot" + subtitle
  2. Summary cards — 5-column table (Channels, Families, Attributes, Categories, Attr. Options) with emoji icons and large count numbers
  3. CHANNELS section — table: Code | Label | Locales | Currencies | Category Tree
  4. FAMILIES section — table: Code | Label | Attributes (count lozenge) | Label Attr | Image Attr
  5. ATTRIBUTES section — table: Code | Label | Type | Group | Scopable | Localizable (checkmark/X emoji)
  6. CATEGORIES section — table: Code | Labels (locale-tagged) | Parent | Updated
  7. ATTRIBUTE OPTIONS section — grouped by parent attribute code, sub-tables: Code | Label | Sort Order
- **Children** = one `SnapshotChildPage` per family, titled "Family: {label} ({code})"

**Family detail pages** (rendered by `render_family_detail_page`):
  1. Title with family label + code badge + subtitle
  2. "Family Configuration" — 3x2 metadata table: Family Code, Label, Parent / Attribute as Label, Attribute as Image, Total Attributes
  3. "Attribute Requirements" — table: Channel | Required Attributes (as `<code>` tags)
  4. "Family Attributes" — enriched table cross-referencing the snapshot's `attributes` array: Attribute Code | Type | Group | Scopable | Localizable | Required (channel names)

**Formatting helpers (lines 837-964):**
- `status_badge(label, count, color)` — Confluence `<ac:structured-macro ac:name="status">` lozenge with "Label: N"
- `status_lozenge(count, color)` — Count-only lozenge
- `info_panel(body_html)` — Confluence info panel macro
- `section_heading(label, count, color)` — Uppercase `<h2>` with count lozenge
- `check_icon(bool)` — Checkmark or X emoji
- `get_code(item)`, `get_label(item)`, `get_string_array(item, field)`, `render_labels_inline(item)`
- `escape_html(s)`, `capitalize(s)`

### `src/confluence.rs` (~325 lines)
Confluence Cloud REST API v1 client with upsert (create-or-update) semantics.
- `ConfluenceConfig { base_url, email, api_token, space_key, parent_page }`
- `ConfluenceClient::publish_page(title, body)` — Upserts under the configured parent page.
- `ConfluenceClient::publish_page_under_id(title, body, parent_id)` — Upserts under a specific parent page ID (used for child pages).
- `upsert_page()` — Searches by title in space, updates (version increment) if found, creates if not.
- Uses HTTP Basic Auth (email + api_token).
- Content published with `"representation": "storage"`.

---

## Snapshot Data Shape

The `snapshot.data` JSONB column contains:

```json
{
  "channels": [
    {
      "code": "cambridge_bioscience",
      "labels": { "en_GB": "Cambridge Bioscience" },
      "locales": ["en_GB"],
      "currencies": ["EUR"],
      "category_tree": "cambridge_tree",
      "conversion_units": {}
    }
  ],
  "families": [
    {
      "code": "antibodies",
      "labels": { "en_GB": "Antibodies" },
      "parent": null,
      "attributes": ["applications", "sku", "..."],
      "attribute_as_image": "product_image",
      "attribute_as_label": "name",
      "attribute_requirements": {
        "cambridge_bioscience": ["sku"]
      }
    }
  ],
  "attributes": [
    {
      "code": "applications",
      "type": "pim_catalog_multiselect",
      "group": "ecom_facets",
      "labels": { "en_GB": "Applications" },
      "unique": false,
      "scopable": false,
      "localizable": false,
      "group_labels": { "en_GB": "Webshop facets" },
      "is_mandatory": false,
      "...many nullable fields..."
    }
  ],
  "categories": [
    {
      "code": "Sanbio",
      "labels": { "en_GB": "Sanbio" },
      "parent": null,
      "updated": "2026-01-07T12:19:05+00:00",
      "validations": { "only_leaves": false, "is_mandatory": false },
      "channel_requirements": []
    }
  ],
  "attribute_options": {
    "tag": [
      { "code": "gst", "labels": { "en_GB": "GST" }, "attribute": "tag", "sort_order": 0 }
    ],
    "host": ["..."],
    "origin": ["..."]
  }
}
```

**Important:** `attribute_options` is a **dictionary** mapping attribute codes to arrays of option objects. All other top-level keys are arrays. The renderer handles this difference explicitly in `render_attribute_options_sections()`.

---

## Confluence Output Format

Output uses [Confluence Storage Format](https://confluence.atlassian.com/doc/confluence-storage-format-790796544.html) — XHTML with Atlassian-specific macro extensions:

| Markup | Usage |
|---|---|
| `<h1>`, `<h2>`, `<h3>` | Page title, section headings, sub-sections |
| `<table data-layout="full-width"><tbody><tr><th>/<td>` | All data tables |
| `<code>` | Identifier codes (renders with monospace grey background in Confluence) |
| `<strong>`, `<em>` | Bold/italic emphasis |
| `<hr/>` | Section dividers |
| `<ac:structured-macro ac:name="status">` | Colored lozenge badges (Green/Red/Yellow/Blue/Purple/Grey) |
| `<ac:structured-macro ac:name="info">` | Info panel (blue box) |
| `<span style="color: red/green">` | Diff old/new value coloring |
| Unicode emoji (checkmark, X) | Boolean true/false indicators in tables |
| Unicode emoji (various) | Summary card icons |

---

## Diff Data Shape

The `diff.data` JSONB column contains:

```json
{
  "attributes": {
    "added": [ { "full attribute object" } ],
    "removed": [ { "full attribute object" } ],
    "changed": [
      {
        "code": "some_attribute",
        "changes": {
          "labels": { "en_GB": { "old": "Old Label", "new": "New Label" } },
          "attributes": { "added": ["new_attr"], "removed": ["old_attr"] }
        }
      }
    ]
  },
  "families": { "added": [], "removed": [], "changed": [] }
}
```

Changes use three patterns detected by `flatten_changes()`:
- **Leaf:** `{ "old": value, "new": value }` -> `FieldChange`
- **Nested sub-diff:** `{ "added": [...], "removed": [...] }` -> `NestedFieldDiff`
- **Nested object:** recurse with dotted path prefix

---

## Database Schema

Tables in `db-schema/database.sql`:

| Table | Purpose | Key Columns |
|---|---|---|
| `akeneo_server` | Akeneo API connection config | id (UUID), name, base_url, client_id/secret, username, password, server_type, status |
| `snapshot` | Full JSON snapshots from Akeneo | id (UUID), akeneo_server_id (FK), label, started_at, completed_at, data (JSONB) |
| `diff` | Computed diffs between two snapshots | id (UUID), snapshot_before_id (FK), snapshot_after_id (FK), data (JSONB) |
| `endpoint_config` | Akeneo API endpoint definitions | id, name, path, blacklist, sort_by, parent_endpoint_id, path_parameter |
| `confluence_config` | Confluence Cloud connection config | id, akeneo_server_id (FK), base_url, username, api_token, space_key, parent_page |

Credential resolution: `snapshot.akeneo_server_id` -> `confluence_config.akeneo_server_id`

---

## UI Design Reference

UI designs for the Confluence output are accessible via the **Pencil MCP**. Use the Pencil MCP tools (`get_editor_state`, `batch_get`, `get_screenshot`) to view the active design. The design contains three screens:

1. **"Akeneo Snapshot Overview"** (node `bi8Au`) — The root overview page with summary cards and all category tables. Primary design reference for the snapshot root page rendering.

2. **"Family Detail -- EC001574"** (node `G8FEh`) — A family detail child page showing breadcrumb navigation, family configuration metadata cards, attribute requirements table, and enriched family attributes table.

3. **"Attribute Detail -- 3d_technology"** (node `McrzN`, disabled) — An attribute detail page design. **Not yet implemented** — deferred for future work.

When working on the renderer output, consult these designs via the Pencil MCP to verify visual structure, column layouts, and information hierarchy.

---

## Key Design Decisions

| Decision | Rationale |
|---|---|
| No template engine | All XHTML built procedurally in Rust. Keeps dependencies minimal but means output changes require recompilation. |
| Untyped JSON (`serde_json::Value`) | Snapshot data shape varies by Akeneo configuration. Generic handling avoids rigid struct definitions. |
| Upsert page semantics | Pages identified by title within a Confluence space. Existing pages updated (version incremented), new pages created. Allows re-running without duplicates. |
| Family cross-referencing | Family detail pages look up each attribute code in the snapshot's `attributes` array to enrich the table with type, group, scopable, localizable data. |
| Emoji for booleans | Confluence Storage Format has limited styling. Checkmark/X emoji render well in Confluence and are visually clear. |
| Summary cards as table | Confluence has no CSS flexbox. Cards simulated as a 5-column single-row table with large text. |
| Category-specific tables | Each category (channels, families, attributes, categories, attr options) has its own purpose-built table with relevant columns, rather than generic property extraction. |

---

## Pending / Future Work

- **Attribute detail pages** — Design exists in Pencil MCP (node `McrzN`, currently disabled). Would create one child page per attribute with identity, properties, configuration details, and group info.
- **Diff rendering redesign** — The diff renderer (`renderer.rs` lines 1-239) has not been redesigned. It still uses the generic `extract_item_properties` / `render_item_table` approach. Consider matching the snapshot design style.
- **README.md update** — The README still describes the old snapshot page structure (one child per category). Should be updated to describe the new structure (overview page + family detail children).

---

## Building & Running

```bash
# Build
cargo build --release

# Run (requires PostgreSQL)
DATABASE_URL=postgres://user:pass@localhost/dbname cargo run

# Docker
docker build -t rust-confluence-documenter .
docker run -e DATABASE_URL="postgres://user:pass@host:5432/dbname" -p 3000:3000 rust-confluence-documenter

# Lint
cargo clippy
```

Environment variables: `DATABASE_URL` (required), `PORT` (default 3000), `RUST_LOG` (default "info").

---

## Example Data

`snapshot-example.json` at the project root contains a real-world snapshot with:
- 2 channels (cambridge_bioscience, szaboscandic)
- 54 families (antibodies, assays_and_elisas, chemicals, proteins, etc.)
- 76 attributes (various types: text, multiselect, textarea, boolean, image, file)
- 61 categories (hierarchical with parent references)
- 34 attribute option groups (tag, host, origin, purity, etc.)

This file is useful for understanding the data shape and testing renderer output locally.
