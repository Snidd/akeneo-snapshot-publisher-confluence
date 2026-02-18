use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::postgres::PgPool;
use sqlx::Row;
use uuid::Uuid;

/// A row from the `diff` table.
#[allow(dead_code)]
pub struct DiffRow {
    pub id: Uuid,
    pub snapshot_before_id: Uuid,
    pub snapshot_after_id: Uuid,
    pub data: serde_json::Value,
}

/// A row from the `snapshot` table.
#[allow(dead_code)]
pub struct SnapshotRow {
    pub id: Uuid,
    pub akeneo_server_id: Uuid,
    pub label: Option<String>,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub data: serde_json::Value,
}

/// Confluence connection configuration from the `confluence_config` table.
pub struct DbConfluenceConfig {
    pub base_url: String,
    pub username: String,
    pub api_token: String,
    pub space_key: String,
    pub parent_page: String,
}

/// Create a connection pool from the DATABASE_URL environment variable.
pub async fn connect() -> Result<PgPool> {
    let database_url =
        std::env::var("DATABASE_URL").context("DATABASE_URL environment variable is required")?;

    PgPool::connect(&database_url)
        .await
        .context("Failed to connect to database")
}

/// Fetch a diff row and both of its related snapshots (before and after).
pub async fn fetch_diff(pool: &PgPool, diff_id: Uuid) -> Result<(DiffRow, SnapshotRow, SnapshotRow)> {
    let row = sqlx::query(
        "SELECT id, snapshot_before_id, snapshot_after_id, data FROM diff WHERE id = $1",
    )
    .bind(diff_id)
    .fetch_one(pool)
    .await
    .with_context(|| format!("Diff not found: {}", diff_id))?;

    let diff_row = DiffRow {
        id: row.get("id"),
        snapshot_before_id: row.get("snapshot_before_id"),
        snapshot_after_id: row.get("snapshot_after_id"),
        data: row.get("data"),
    };

    let (before, after) = tokio::try_join!(
        fetch_snapshot(pool, diff_row.snapshot_before_id),
        fetch_snapshot(pool, diff_row.snapshot_after_id),
    )?;

    Ok((diff_row, before, after))
}

/// Fetch a single snapshot row by ID.
pub async fn fetch_snapshot(pool: &PgPool, snapshot_id: Uuid) -> Result<SnapshotRow> {
    let row = sqlx::query(
        "SELECT id, akeneo_server_id, label, started_at, completed_at, data FROM snapshot WHERE id = $1",
    )
    .bind(snapshot_id)
    .fetch_one(pool)
    .await
    .with_context(|| format!("Snapshot not found: {}", snapshot_id))?;

    Ok(SnapshotRow {
        id: row.get("id"),
        akeneo_server_id: row.get("akeneo_server_id"),
        label: row.get("label"),
        started_at: row.get("started_at"),
        completed_at: row.get("completed_at"),
        data: row.get("data"),
    })
}

/// Fetch the Confluence configuration for the akeneo_server linked to a snapshot.
pub async fn fetch_confluence_config(
    pool: &PgPool,
    akeneo_server_id: Uuid,
) -> Result<DbConfluenceConfig> {
    let row = sqlx::query(
        "SELECT base_url, username, api_token, space_key, parent_page FROM confluence_config WHERE akeneo_server_id = $1",
    )
    .bind(akeneo_server_id)
    .fetch_one(pool)
    .await
    .with_context(|| {
        format!(
            "No Confluence configuration found for akeneo_server: {}",
            akeneo_server_id
        )
    })?;

    Ok(DbConfluenceConfig {
        base_url: row.get("base_url"),
        username: row.get("username"),
        api_token: row.get("api_token"),
        space_key: row.get("space_key"),
        parent_page: row.get("parent_page"),
    })
}
