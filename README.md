# Confluence Documenter

A web service that reads Akeneo PIM snapshot and diff data from a PostgreSQL database, renders it as Confluence storage format (XHTML) pages, and publishes them to Confluence Cloud.

## Prerequisites

- **Rust** (1.85+ / edition 2024) — [install via rustup](https://rustup.rs/)
- **PostgreSQL** — a running instance with the schema from `../db-schema/database.sql` applied

## Database Schema

The application reads from the following tables (see `../db-schema/database.sql` for the full schema):

| Table | Purpose |
|---|---|
| `akeneo_server` | Akeneo API server connection details |
| `snapshot` | Full JSON snapshots captured from an Akeneo server |
| `diff` | Computed differences between two snapshots |
| `confluence_config` | Confluence Cloud connection details, linked to an Akeneo server |

The data flow for resolving Confluence credentials is:
`diff` → `snapshot` → `akeneo_server` → `confluence_config`

## Building

```bash
cargo build --release
```

The compiled binary will be at `target/release/rust-confluence-documenter`.

## Configuration

| Variable | Required | Description |
|---|---|---|
| `DATABASE_URL` | Yes | PostgreSQL connection string (e.g. `postgres://user:pass@localhost:5432/dbname`) |
| `PORT` | No | HTTP server port (defaults to `3000`) |
| `RUST_LOG` | No | Log level filter (defaults to `info`). See [tracing-subscriber docs](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/struct.EnvFilter.html) for syntax. |

Confluence connection details (base URL, credentials, space key, parent page) are read from the `confluence_config` table in the database, not from environment variables.

## Usage

The application starts an HTTP server with two endpoints. Both endpoints fetch data from the database, render Confluence pages, publish them, and return the resulting page URL.

```bash
DATABASE_URL=postgres://user:pass@localhost/mydb cargo run
```

### Endpoints

#### `GET /api/snapshot/{id}`

Fetches a snapshot by UUID, renders a multi-page Confluence page tree (root page + one child page per category), publishes all pages, and returns the root page URL.

```bash
curl http://localhost:3000/api/snapshot/550e8400-e29b-41d4-a716-446655440000
```

#### `GET /api/diff/{id}`

Fetches a diff by UUID (along with its before/after snapshots), renders a single Confluence diff page, publishes it, and returns the page URL.

```bash
curl http://localhost:3000/api/diff/550e8400-e29b-41d4-a716-446655440000
```

### Response Format

**Success (200):**

```json
{
  "status": "ok",
  "page_url": "https://your-instance.atlassian.net/wiki/spaces/SPACE/pages/12345/Page+Title"
}
```

**Not Found (404):**

```json
{
  "status": "error",
  "message": "Snapshot not found: 550e8400-e29b-41d4-a716-446655440000"
}
```

**Internal Server Error (500):**

```json
{
  "status": "error",
  "message": "Failed to publish root page to Confluence: ..."
}
```

## Project Structure

```
src/
  main.rs         HTTP server setup, route handlers (Axum)
  db.rs           PostgreSQL queries (diff, snapshot, confluence_config)
  diff.rs         Parses diff JSON data into structured report types
  renderer.rs     Renders diffs and snapshots as Confluence storage format (XHTML)
  confluence.rs   Confluence Cloud REST API client (search, create, update pages)
```

## Output Format

All rendered content uses [Confluence Storage Format](https://confluence.atlassian.com/doc/confluence-storage-format-790796544.html) (XHTML), including:

- Standard HTML elements (`<h2>`, `<table>`, `<ul>`, `<code>`, etc.)
- `<ac:structured-macro ac:name="status">` colored status lozenges
- `<ac:structured-macro ac:name="info">` info panels
- `<span style="color: red/green">` colored text for old/new diff values

When publishing, the API sends content with `"representation": "storage"` which Confluence renders directly.
