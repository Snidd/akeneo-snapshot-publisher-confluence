# Confluence Documenter

A CLI tool that reads Akeneo PIM snapshot and diff data from a PostgreSQL database and renders it as Confluence Wiki Markup pages. Output is printed to stdout by default, with an option to publish directly to Confluence Cloud.

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

The only required environment variable is:

| Variable | Description |
|---|---|
| `DATABASE_URL` | PostgreSQL connection string (e.g. `postgres://user:pass@localhost:5432/dbname`) |

Confluence connection details (base URL, credentials, space key, parent page) are read from the `confluence_config` table in the database, not from environment variables.

## Usage

The tool has two subcommands: `diff` and `snapshot`.

### Print a diff

Fetches a diff by its UUID, renders it as Confluence Wiki Markup, and prints to stdout:

```bash
DATABASE_URL=postgres://user:pass@localhost/mydb \
  cargo run -- diff <diff-uuid>
```

### Print a snapshot

Fetches a snapshot by its UUID, renders it as a categorized item listing, and prints to stdout:

```bash
DATABASE_URL=postgres://user:pass@localhost/mydb \
  cargo run -- snapshot <snapshot-uuid>
```

### Publish to Confluence

Add the `--publish` flag to any subcommand to also push the rendered page to Confluence Cloud. The tool will create a new page or update an existing one with the same title (upsert behavior):

```bash
# Publish a diff
DATABASE_URL=postgres://user:pass@localhost/mydb \
  cargo run -- diff <diff-uuid> --publish

# Publish a snapshot
DATABASE_URL=postgres://user:pass@localhost/mydb \
  cargo run -- snapshot <snapshot-uuid> --publish
```

### Help

```bash
cargo run -- --help
cargo run -- diff --help
cargo run -- snapshot --help
```

## Project Structure

```
src/
  main.rs         CLI entry point and subcommand handling
  db.rs           PostgreSQL queries (diff, snapshot, confluence_config)
  diff.rs         Parses diff JSON data into structured report types
  renderer.rs     Renders diffs and snapshots as Confluence Wiki Markup
  confluence.rs   Confluence Cloud REST API client (async, wiki representation)
```

## Output Format

All output uses [Confluence Wiki Markup](https://confluence.atlassian.com/doc/confluence-wiki-markup-251003035.html) syntax, including:

- `h2.` / `h3.` headings
- `||heading||` / `|cell|` tables
- `{{monospace}}` code formatting
- `{status:title=...|colour=...}` status lozenges
- `{color:red}...{color}` colored text for old/new diff values
- `{info}...{info}` info panels

When publishing, the API sends content with `"representation": "wiki"` which Confluence converts to storage format server-side.
