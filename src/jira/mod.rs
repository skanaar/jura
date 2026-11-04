//! Jira REST client: data types, HTTP client and response parsing.

mod client;
mod parse;

use serde::{Deserialize, Serialize};

pub use client::JiraClient;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Issue {
    pub key: String,
    pub summary: String,
    pub status: String,
    /// Jira status category key: `new`, `indeterminate` or `done`.
    pub status_category: String,
    pub issue_type: String,
    pub priority: Option<String>,
    pub assignee: Option<String>,
    pub reporter: Option<String>,
    pub created: String,
    pub updated: String,
    pub labels: Vec<String>,
    #[serde(default)]
    pub fix_versions: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Comment {
    pub author: String,
    pub created: String,
    pub body: String,
    /// Optimistically added locally, not yet confirmed by the server.
    #[serde(default)]
    pub pending: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transition {
    pub id: String,
    pub name: String,
    pub to: String,
    pub to_category: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IssueDetail {
    pub issue: Issue,
    pub description: String,
    pub comments: Vec<Comment>,
    pub transitions: Vec<Transition>,
}

#[derive(Clone, Debug)]
pub struct Me {
    /// `accountId` on Cloud, `name` on Server.
    pub id: String,
    pub display_name: String,
}
