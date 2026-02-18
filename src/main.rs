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

    // Render the page
    let (title, body) = renderer::render_snapshot_page(snapshot.label.as_deref(), &snapshot.data);

    println!("\n--- Page Title ---");
    println!("{}", title);
    println!("\n--- Page Body (Confluence Wiki Markup) ---");
    println!("{}", body);

    if publish {
        let confluence_config =
            db::fetch_confluence_config(pool, snapshot.akeneo_server_id).await?;
        let config = confluence::ConfluenceConfig::from_db(confluence_config);
        let client = confluence::ConfluenceClient::new(config);

        println!("\n--- Publishing to Confluence ---");
        client.publish_page(&title, &body).await?;
        println!("Done!");
    }

    Ok(())
}
