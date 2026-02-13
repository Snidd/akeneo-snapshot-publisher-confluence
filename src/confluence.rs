use anyhow::{bail, Context, Result};
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use serde::Deserialize;

/// Configuration for connecting to Confluence Cloud.
pub struct ConfluenceConfig {
    pub base_url: String,
    pub email: String,
    pub api_token: String,
    pub space_key: String,
}

impl ConfluenceConfig {
    /// Build config from environment variables.
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            base_url: std::env::var("CONFLUENCE_URL")
                .context("CONFLUENCE_URL environment variable is required")?,
            email: std::env::var("CONFLUENCE_EMAIL")
                .context("CONFLUENCE_EMAIL environment variable is required")?,
            api_token: std::env::var("CONFLUENCE_API_TOKEN")
                .context("CONFLUENCE_API_TOKEN environment variable is required")?,
            space_key: std::env::var("CONFLUENCE_SPACE_KEY")
                .context("CONFLUENCE_SPACE_KEY environment variable is required")?,
        })
    }
}

/// Confluence REST API client.
pub struct ConfluenceClient {
    client: Client,
    config: ConfluenceConfig,
}

#[derive(Deserialize, Debug)]
struct SearchResults {
    results: Vec<PageResult>,
}

#[derive(Deserialize, Debug)]
struct PageResult {
    id: String,
    version: Option<VersionInfo>,
}

#[derive(Deserialize, Debug)]
struct VersionInfo {
    number: u64,
}

#[derive(Deserialize, Debug)]
struct CreatePageResponse {
    id: String,
    #[serde(rename = "_links")]
    links: Option<PageLinks>,
}

#[derive(Deserialize, Debug)]
struct PageLinks {
    #[serde(rename = "webui")]
    webui: Option<String>,
}

impl ConfluenceClient {
    pub fn new(config: ConfluenceConfig) -> Self {
        Self {
            client: Client::new(),
            config,
        }
    }

    /// Search for an existing page by title in the configured space.
    /// Returns the page ID and current version number if found.
    fn find_page(&self, title: &str) -> Result<Option<(String, u64)>> {
        let url = format!(
            "{}/wiki/rest/api/content",
            self.config.base_url.trim_end_matches('/')
        );

        let resp = self
            .client
            .get(&url)
            .basic_auth(&self.config.email, Some(&self.config.api_token))
            .header(ACCEPT, "application/json")
            .query(&[
                ("title", title),
                ("spaceKey", &self.config.space_key),
                ("expand", "version"),
            ])
            .send()
            .context("Failed to search for existing page")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            bail!(
                "Confluence search request failed (HTTP {}): {}",
                status,
                body
            );
        }

        let results: SearchResults = resp.json().context("Failed to parse search response")?;

        if let Some(page) = results.results.first() {
            let version = page.version.as_ref().map(|v| v.number).unwrap_or(1);
            Ok(Some((page.id.clone(), version)))
        } else {
            Ok(None)
        }
    }

    /// Create a new Confluence page.
    fn create_page(
        &self,
        title: &str,
        body_html: &str,
        parent_page_id: Option<&str>,
    ) -> Result<String> {
        let url = format!(
            "{}/wiki/rest/api/content",
            self.config.base_url.trim_end_matches('/')
        );

        let mut page_json = serde_json::json!({
            "type": "page",
            "title": title,
            "space": {
                "key": &self.config.space_key
            },
            "body": {
                "storage": {
                    "value": body_html,
                    "representation": "storage"
                }
            }
        });

        if let Some(parent_id) = parent_page_id {
            page_json["ancestors"] = serde_json::json!([{ "id": parent_id }]);
        }

        let resp = self
            .client
            .post(&url)
            .basic_auth(&self.config.email, Some(&self.config.api_token))
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "application/json")
            .json(&page_json)
            .send()
            .context("Failed to create Confluence page")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            bail!("Confluence create page failed (HTTP {}): {}", status, body);
        }

        let result: CreatePageResponse = resp.json().context("Failed to parse create response")?;

        let web_url = result
            .links
            .and_then(|l| l.webui)
            .map(|path| {
                format!(
                    "{}/wiki{}",
                    self.config.base_url.trim_end_matches('/'),
                    path
                )
            })
            .unwrap_or_else(|| {
                format!(
                    "{}/wiki/spaces/{}/pages/{}",
                    self.config.base_url.trim_end_matches('/'),
                    self.config.space_key,
                    result.id
                )
            });

        println!("Created new page: {}", web_url);
        Ok(result.id)
    }

    /// Update an existing Confluence page.
    fn update_page(
        &self,
        page_id: &str,
        title: &str,
        body_html: &str,
        current_version: u64,
    ) -> Result<String> {
        let url = format!(
            "{}/wiki/rest/api/content/{}",
            self.config.base_url.trim_end_matches('/'),
            page_id
        );

        let page_json = serde_json::json!({
            "type": "page",
            "title": title,
            "version": {
                "number": current_version + 1
            },
            "body": {
                "storage": {
                    "value": body_html,
                    "representation": "storage"
                }
            }
        });

        let resp = self
            .client
            .put(&url)
            .basic_auth(&self.config.email, Some(&self.config.api_token))
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "application/json")
            .json(&page_json)
            .send()
            .context("Failed to update Confluence page")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            bail!("Confluence update page failed (HTTP {}): {}", status, body);
        }

        let result: CreatePageResponse = resp.json().context("Failed to parse update response")?;

        let web_url = result
            .links
            .and_then(|l| l.webui)
            .map(|path| {
                format!(
                    "{}/wiki{}",
                    self.config.base_url.trim_end_matches('/'),
                    path
                )
            })
            .unwrap_or_else(|| {
                format!(
                    "{}/wiki/spaces/{}/pages/{}",
                    self.config.base_url.trim_end_matches('/'),
                    self.config.space_key,
                    result.id
                )
            });

        println!(
            "Updated existing page (v{}): {}",
            current_version + 1,
            web_url
        );
        Ok(result.id)
    }

    /// Create or update a Confluence page with the given title and body.
    /// If a page with the same title already exists in the space, it will be updated.
    /// Otherwise, a new page will be created.
    pub fn publish_page(
        &self,
        title: &str,
        body_html: &str,
        parent_page_id: Option<&str>,
    ) -> Result<String> {
        println!("Searching for existing page: \"{}\"...", title);

        match self.find_page(title)? {
            Some((page_id, version)) => {
                println!(
                    "Found existing page (id={}, version={}). Updating...",
                    page_id, version
                );
                self.update_page(&page_id, title, body_html, version)
            }
            None => {
                println!("No existing page found. Creating new page...");
                self.create_page(title, body_html, parent_page_id)
            }
        }
    }
}
