use anyhow::{bail, Context, Result};
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use reqwest::Client;
use serde::Deserialize;

use crate::db::DbConfluenceConfig;

/// Configuration for connecting to Confluence Cloud.
pub struct ConfluenceConfig {
    pub base_url: String,
    pub email: String,
    pub api_token: String,
    pub space_key: String,
    pub parent_page: String,
}

impl ConfluenceConfig {
    /// Build config from database configuration.
    pub fn from_db(db_config: DbConfluenceConfig) -> Self {
        Self {
            base_url: db_config.base_url,
            email: db_config.username,
            api_token: db_config.api_token,
            space_key: db_config.space_key,
            parent_page: db_config.parent_page,
        }
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
    async fn find_page(&self, title: &str) -> Result<Option<(String, u64)>> {
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
            .await
            .context("Failed to search for existing page")?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!(
                "Confluence search request failed (HTTP {}): {}",
                status,
                body
            );
        }

        let results: SearchResults = resp.json().await.context("Failed to parse search response")?;

        if let Some(page) = results.results.first() {
            let version = page.version.as_ref().map(|v| v.number).unwrap_or(1);
            Ok(Some((page.id.clone(), version)))
        } else {
            Ok(None)
        }
    }

    /// Create a new Confluence page using wiki markup representation.
    async fn create_page(&self, title: &str, body_wiki: &str) -> Result<String> {
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
                "wiki": {
                    "value": body_wiki,
                    "representation": "wiki"
                }
            }
        });

        // Always nest under the configured parent page (resolve title to numeric ID)
        if !self.config.parent_page.is_empty() {
            let parent_id = self
                .find_page(&self.config.parent_page)
                .await?
                .map(|(id, _version)| id)
                .with_context(|| {
                    format!(
                        "Parent page '{}' not found in space '{}'",
                        self.config.parent_page, self.config.space_key
                    )
                })?;
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
            .await
            .context("Failed to create Confluence page")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("Confluence create page failed (HTTP {}): {}", status, body);
        }

        let result: CreatePageResponse =
            resp.json().await.context("Failed to parse create response")?;

        let web_url = self.build_web_url(&result);
        println!("Created new page: {}", web_url);
        Ok(result.id)
    }

    /// Update an existing Confluence page using wiki markup representation.
    async fn update_page(
        &self,
        page_id: &str,
        title: &str,
        body_wiki: &str,
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
                "wiki": {
                    "value": body_wiki,
                    "representation": "wiki"
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
            .await
            .context("Failed to update Confluence page")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("Confluence update page failed (HTTP {}): {}", status, body);
        }

        let result: CreatePageResponse =
            resp.json().await.context("Failed to parse update response")?;

        let web_url = self.build_web_url(&result);
        println!(
            "Updated existing page (v{}): {}",
            current_version + 1,
            web_url
        );
        Ok(result.id)
    }

    /// Create or update a Confluence page with the given title and wiki markup body.
    /// If a page with the same title already exists in the space, it will be updated.
    /// Otherwise, a new page will be created.
    pub async fn publish_page(&self, title: &str, body_wiki: &str) -> Result<String> {
        println!("Searching for existing page: \"{}\"...", title);

        match self.find_page(title).await? {
            Some((page_id, version)) => {
                println!(
                    "Found existing page (id={}, version={}). Updating...",
                    page_id, version
                );
                self.update_page(&page_id, title, body_wiki, version).await
            }
            None => {
                println!("No existing page found. Creating new page...");
                self.create_page(title, body_wiki).await
            }
        }
    }

    /// Build the web URL for a page from its API response.
    fn build_web_url(&self, response: &CreatePageResponse) -> String {
        response
            .links
            .as_ref()
            .and_then(|l| l.webui.as_ref())
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
                    response.id
                )
            })
    }
}
