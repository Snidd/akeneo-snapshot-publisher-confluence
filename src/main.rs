mod confluence;
mod diff;
mod renderer;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

/// Generate Confluence release notes from a JSON diff file.
///
/// Reads a diff JSON file describing added, removed, and changed items across
/// categories (e.g. attributes, families), and publishes a formatted release
/// notes page to Confluence Cloud.
///
/// Required environment variables:
///   CONFLUENCE_URL        — Base URL (e.g. https://yoursite.atlassian.net)
///   CONFLUENCE_EMAIL      — API user email
///   CONFLUENCE_API_TOKEN  — API token
///   CONFLUENCE_SPACE_KEY  — Space key (e.g. DOC)
#[derive(Parser, Debug)]
#[command(name = "confluence-release-notes", about, disable_version_flag = true)]
struct Cli {
    /// Path to the diff JSON file
    #[arg(short, long, default_value = "diff.json")]
    file: PathBuf,

    /// Release version (e.g. "1.2.0")
    #[arg(short, long)]
    version: String,

    /// Release description
    #[arg(short, long)]
    description: String,

    /// Parent page ID to nest the release notes under (optional)
    #[arg(short, long)]
    parent_page_id: Option<String>,

    /// Dry run: render the page and print to stdout without publishing
    #[arg(long, default_value_t = false)]
    dry_run: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Parse the diff file
    println!("Reading diff from: {}", cli.file.display());
    let report = diff::parse_diff_file(&cli.file)?;

    // Print a quick summary
    for (category, diff) in &report {
        println!(
            "  {}: {} added, {} removed, {} changed",
            category,
            diff.added.len(),
            diff.removed.len(),
            diff.changed.len()
        );
    }

    // Render the Confluence page body
    let title = renderer::page_title(&cli.version);
    let body = renderer::render_page(&cli.version, &cli.description, &report);

    if cli.dry_run {
        println!("\n--- Page Title ---");
        println!("{}", title);
        println!("\n--- Page Body (Confluence Storage Format) ---");
        println!("{}", body);
        println!("\n--- Dry run complete. No page was published. ---");
        return Ok(());
    }

    // Publish to Confluence
    let config = confluence::ConfluenceConfig::from_env()?;
    let client = confluence::ConfluenceClient::new(config);

    client.publish_page(&title, &body, cli.parent_page_id.as_deref())?;

    println!("Done!");
    Ok(())
}
