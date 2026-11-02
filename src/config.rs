use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

pub const EXAMPLE: &str = include_str!("../config.example.toml");

#[derive(Debug, Deserialize)]
pub struct Config {
    /// Project key, substituted for `{project}` in filter JQL.
    pub project: String,
    /// How often all filters are revalidated in the background.
    #[serde(default = "default_refresh")]
    pub refresh_interval_secs: u64,
    /// Cached data older than this is revalidated when it is shown.
    #[serde(default = "default_stale")]
    pub stale_after_secs: u64,
    /// Upper bound on issues fetched per filter.
    #[serde(default = "default_max")]
    pub max_results: usize,
    /// Issue type pre-selected when creating an issue.
    #[serde(default = "default_issue_type")]
    pub default_issue_type: String,
    pub jira: JiraConfig,
    #[serde(default)]
    pub filters: Vec<FilterConfig>,
    /// Display text per Jira status name, e.g. `"In Progress" = "WIP"`.
    /// Keys are matched case-insensitively.
    #[serde(default)]
    pub status_names: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct JiraConfig {
    pub url: String,
    /// Account e-mail (Cloud) or username (Server). Omit to use bearer-token auth.
    #[serde(default, alias = "username")]
    pub email: Option<String>,
    pub api_token: Option<String>,
    /// Environment variable holding the token, used when `api_token` is absent.
    pub api_token_env: Option<String>,
    #[serde(default)]
    pub deployment: Deployment,
}

#[derive(Debug, Deserialize, Default, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Deployment {
    #[default]
    Cloud,
    Server,
}

#[derive(Debug, Deserialize, Clone)]
pub struct FilterConfig {
    pub name: String,
    pub jql: String,
}

fn default_refresh() -> u64 {
    60
}
fn default_stale() -> u64 {
    30
}
fn default_issue_type() -> String {
    "Task".into()
}
fn default_max() -> usize {
    300
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading config {}", path.display()))?;
        let mut cfg: Config =
            toml::from_str(&text).with_context(|| format!("parsing config {}", path.display()))?;

        cfg.jira.url = cfg.jira.url.trim_end_matches('/').to_string();
        cfg.max_results = cfg.max_results.max(1);
        cfg.refresh_interval_secs = cfg.refresh_interval_secs.max(5);
        if cfg.filters.is_empty() {
            cfg.filters.push(FilterConfig {
                name: "Open".into(),
                jql: "project = {project} AND statusCategory != Done ORDER BY updated DESC".into(),
            });
        }
        for f in &mut cfg.filters {
            f.jql = f.jql.replace("{project}", &cfg.project);
        }
        cfg.status_names = cfg.status_names.into_iter().map(|(k, v)| (k.to_lowercase(), v)).collect();
        Ok(cfg)
    }

    /// The configured display text for a Jira status, or the status itself.
    pub fn status_label<'a>(&'a self, status: &'a str) -> &'a str {
        self.status_names.get(&status.to_lowercase()).map_or(status, String::as_str)
    }

    pub fn token(&self) -> Result<String> {
        if let Some(t) = &self.jira.api_token {
            return Ok(t.clone());
        }
        let var = self.jira.api_token_env.as_deref().unwrap_or("JIRA_API_TOKEN");
        match std::env::var(var) {
            Ok(t) if !t.is_empty() => Ok(t),
            _ => bail!("no Jira token: set `jira.api_token` in the config or the {var} environment variable"),
        }
    }

    /// Identifies the account + site, so caches for different logins never mix.
    pub fn cache_scope(&self) -> String {
        format!("{}|{}", self.jira.url, self.jira.email.as_deref().unwrap_or(""))
    }
}

/// `--config`, then `$JURA_CONFIG`, then `./jura.toml`, then the platform config dir.
pub fn resolve_path(explicit: Option<PathBuf>) -> PathBuf {
    if let Some(p) = explicit {
        return p;
    }
    if let Ok(p) = std::env::var("JURA_CONFIG") {
        return PathBuf::from(p);
    }
    let local = PathBuf::from("jura.toml");
    if local.exists() {
        return local;
    }
    default_path()
}

pub fn default_path() -> PathBuf {
    directories::ProjectDirs::from("", "", "jura")
        .map(|d| d.config_dir().join("config.toml"))
        .unwrap_or_else(|| PathBuf::from("jura.toml"))
}
