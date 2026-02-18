mod confluence;
mod db;
mod diff;
mod renderer;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::Serialize;
use sqlx::PgPool;
use tower_http::trace::TraceLayer;
use tracing::{error, info};
use uuid::Uuid;

/// Shared application state passed to all handlers.
#[derive(Clone)]
struct AppState {
    pool: PgPool,
}

/// JSON response returned by both endpoints on success.
#[derive(Serialize)]
struct SuccessResponse {
    status: &'static str,
    page_url: String,
}

/// JSON response returned on errors.
#[derive(Serialize)]
struct ErrorResponse {
    status: &'static str,
    message: String,
}

impl ErrorResponse {
    fn new(message: impl Into<String>) -> Self {
        Self {
            status: "error",
            message: message.into(),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing (respects RUST_LOG env var, defaults to info)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let pool = db::connect().await?;
    let state = AppState { pool };

    let app = Router::new()
        .route("/api/snapshot/{id}", get(handle_snapshot))
        .route("/api/diff/{id}", get(handle_diff))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    info!("Listening on 0.0.0.0:{}", port);
    axum::serve(listener, app).await?;

    Ok(())
}

/// GET /api/snapshot/:id
///
/// Fetches a snapshot from the database, renders it as Confluence pages,
/// publishes all pages (root + children), and returns the root page URL.
async fn handle_snapshot(
    State(state): State<AppState>,
    Path(snapshot_id): Path<Uuid>,
) -> impl IntoResponse {
    info!("Processing snapshot: {}", snapshot_id);

    // 1. Fetch snapshot from DB
    let snapshot = match db::fetch_snapshot(&state.pool, snapshot_id).await {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to fetch snapshot {}: {:#}", snapshot_id, e);
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new(format!(
                    "Snapshot not found: {}",
                    snapshot_id
                ))),
            )
                .into_response();
        }
    };

    // 2. Render multi-page snapshot tree
    let page_tree = renderer::render_snapshot_pages(snapshot.label.as_deref(), &snapshot.data);

    // 3. Get Confluence config and build client
    let confluence_config =
        match db::fetch_confluence_config(&state.pool, snapshot.akeneo_server_id).await {
            Ok(c) => c,
            Err(e) => {
                error!(
                    "Failed to fetch Confluence config for server {}: {:#}",
                    snapshot.akeneo_server_id, e
                );
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new(format!(
                        "Failed to fetch Confluence configuration: {}",
                        e
                    ))),
                )
                    .into_response();
            }
        };

    let config = confluence::ConfluenceConfig::from_db(confluence_config);
    let client = confluence::ConfluenceClient::new(config);

    // 4. Publish root page
    let root_result = match client
        .publish_page(&page_tree.root_title, &page_tree.root_body)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            error!("Failed to publish root page: {:#}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!(
                    "Failed to publish root page to Confluence: {}",
                    e
                ))),
            )
                .into_response();
        }
    };

    info!(
        "Root page '{}' published (id={})",
        page_tree.root_title, root_result.page_id
    );

    // 5. Publish each child page under the root page
    for child in &page_tree.children {
        match client
            .publish_page_under_id(&child.title, &child.body, &root_result.page_id)
            .await
        {
            Ok(child_result) => {
                info!(
                    "Child page '{}' published (id={})",
                    child.title, child_result.page_id
                );
            }
            Err(e) => {
                error!("Failed to publish child page '{}': {:#}", child.title, e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new(format!(
                        "Failed to publish child page '{}' to Confluence: {}",
                        child.title, e
                    ))),
                )
                    .into_response();
            }
        }
    }

    // 6. Return the root page URL
    (
        StatusCode::OK,
        Json(SuccessResponse {
            status: "ok",
            page_url: root_result.web_url,
        }),
    )
        .into_response()
}

/// GET /api/diff/:id
///
/// Fetches a diff and its associated snapshots from the database, renders
/// a Confluence diff page, publishes it, and returns the page URL.
async fn handle_diff(
    State(state): State<AppState>,
    Path(diff_id): Path<Uuid>,
) -> impl IntoResponse {
    info!("Processing diff: {}", diff_id);

    // 1. Fetch diff and both snapshots
    let (diff_row, before_snapshot, after_snapshot) =
        match db::fetch_diff(&state.pool, diff_id).await {
            Ok(data) => data,
            Err(e) => {
                error!("Failed to fetch diff {}: {:#}", diff_id, e);
                return (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse::new(format!("Diff not found: {}", diff_id))),
                )
                    .into_response();
            }
        };

    // 2. Parse the diff data
    let report = match diff::parse_diff_data(&diff_row.data) {
        Ok(r) => r,
        Err(e) => {
            error!("Failed to parse diff data for {}: {:#}", diff_id, e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!(
                    "Failed to parse diff data: {}",
                    e
                ))),
            )
                .into_response();
        }
    };

    // Log summary
    for (category, cat_diff) in &report {
        info!(
            "  {}: {} added, {} removed, {} changed",
            category,
            cat_diff.added.len(),
            cat_diff.removed.len(),
            cat_diff.changed.len()
        );
    }

    // 3. Render the diff page
    let (title, body) = renderer::render_diff_page(
        before_snapshot.label.as_deref(),
        after_snapshot.label.as_deref(),
        &report,
    );

    // 4. Get Confluence config and build client
    let confluence_config =
        match db::fetch_confluence_config(&state.pool, after_snapshot.akeneo_server_id).await {
            Ok(c) => c,
            Err(e) => {
                error!(
                    "Failed to fetch Confluence config for server {}: {:#}",
                    after_snapshot.akeneo_server_id, e
                );
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new(format!(
                        "Failed to fetch Confluence configuration: {}",
                        e
                    ))),
                )
                    .into_response();
            }
        };

    let config = confluence::ConfluenceConfig::from_db(confluence_config);
    let client = confluence::ConfluenceClient::new(config);

    // 5. Publish the diff page
    let result = match client.publish_page(&title, &body).await {
        Ok(r) => r,
        Err(e) => {
            error!("Failed to publish diff page: {:#}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(format!(
                    "Failed to publish diff page to Confluence: {}",
                    e
                ))),
            )
                .into_response();
        }
    };

    info!("Diff page '{}' published (id={})", title, result.page_id);

    // 6. Return the page URL
    (
        StatusCode::OK,
        Json(SuccessResponse {
            status: "ok",
            page_url: result.web_url,
        }),
    )
        .into_response()
}
