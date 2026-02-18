mod confluence;
mod db;
mod diff;
mod renderer;

use anyhow::Result;
use clap::{Parser, Subcommand};
use uuid::Uuid;

/// Generate Confluence documentation from Akeneo snapshot and diff data.
///
/// Reads data from a PostgreSQL database (via DATABASE_URL env var) and renders
/// Confluence Wiki Markup pages for diffs or snapshots.
///
/// Prints rendered output to stdout by default. Use --publish to also push
/// the page to Confluence Cloud (using configuration from the database).
#[derive(Parser, Debug)]
#[command(name = "confluence-documenter")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Render a diff between two snapshots
    Diff {
        /// The UUID of the diff to render
        diff_id: Uuid,

        /// Publish the rendered page to Confluence
        #[arg(long, default_value_t = false)]
        publish: bool,
    },

    /// Render a snapshot
    Snapshot {
        /// The UUID of the snapshot to render
        snapshot_id: Uuid,

        /// Publish the rendered page to Confluence
        #[arg(long, default_value_t = false)]
        publish: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let pool = db::connect().await?;

    match cli.command {
        Command::Diff { diff_id, publish } => {
            handle_diff(&pool, diff_id, publish).await?;
        }
        Command::Snapshot {
            snapshot_id,
            publish,
        } => {
            handle_snapshot(&pool, snapshot_id, publish).await?;
        }
    }

    Ok(())
}

async fn handle_diff(pool: &sqlx::PgPool, diff_id: Uuid, publish: bool) -> Result<()> {
    println!("Fetching diff: {}", diff_id);
    let (diff_row, before_snapshot, after_snapshot) = db::fetch_diff(pool, diff_id).await?;

    // Parse the diff data
    let report = diff::parse_diff_data(&diff_row.data)?;

    // Print summary
    for (category, cat_diff) in &report {
        println!(
            "  {}: {} added, {} removed, {} changed",
            category,
            cat_diff.added.len(),
            cat_diff.removed.len(),
            cat_diff.changed.len()
        );
    }

    // Render the page
    let (title, body) = renderer::render_diff_page(
        before_snapshot.label.as_deref(),
        after_snapshot.label.as_deref(),
        &report,
    );

    println!("\n--- Page Title ---");
    println!("{}", title);
    println!("\n--- Page Body (Confluence Wiki Markup) ---");
    println!("{}", body);

    if publish {
        let confluence_config =
            db::fetch_confluence_config(pool, after_snapshot.akeneo_server_id).await?;
        let config = confluence::ConfluenceConfig::from_db(confluence_config);
        let client = confluence::ConfluenceClient::new(config);

        println!("\n--- Publishing to Confluence ---");
        client.publish_page(&title, &body).await?;
        println!("Done!");
    }

    Ok(())
}

async fn handle_snapshot(pool: &sqlx::PgPool, snapshot_id: Uuid, publish: bool) -> Result<()> {
    println!("Fetching snapshot: {}", snapshot_id);
    let snapshot = db::fetch_snapshot(pool, snapshot_id).await?;

    // Render multi-page snapshot tree
    let page_tree =
        renderer::render_snapshot_pages(snapshot.label.as_deref(), &snapshot.data);

    // Print root page
    println!("\n--- Root Page: {} ---", page_tree.root_title);
    println!("{}", page_tree.root_body);

    // Print child pages
    for child in &page_tree.children {
        println!("\n--- Child Page: {} ---", child.title);
        println!("{}", child.body);
    }

    if publish {
        let confluence_config =
            db::fetch_confluence_config(pool, snapshot.akeneo_server_id).await?;
        let config = confluence::ConfluenceConfig::from_db(confluence_config);
        let client = confluence::ConfluenceClient::new(config);

        println!("\n--- Publishing to Confluence ---");

        // 1. Publish the root "Current model" page under the configured parent
        let root_page_id = client
            .publish_page(&page_tree.root_title, &page_tree.root_body)
            .await?;
        println!(
            "Root page '{}' published (id={})",
            page_tree.root_title, root_page_id
        );

        // 2. Publish each category child page under the root page
        for child in &page_tree.children {
            let child_id = client
                .publish_page_under_id(&child.title, &child.body, &root_page_id)
                .await?;
            println!(
                "Child page '{}' published (id={})",
                child.title, child_id
            );
        }

        println!("Done!");
    }

    Ok(())
}
