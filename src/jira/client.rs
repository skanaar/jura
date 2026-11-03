use std::time::Duration;

use anyhow::{Result, bail};
use reqwest::{Client, Method, RequestBuilder, header};
use serde_json::{Value, json};
use tokio::sync::OnceCell;

use super::parse::{error_message, markdown_blocks, parse_comment, parse_issue, parse_transitions, rich_text};
use super::{Issue, IssueDetail, Me, Transition};
use crate::config::{Config, Deployment};

/// Fields requested for list views. Kept small so searches stay fast.
const LIST_FIELDS: &[&str] = &[
    "summary", "status", "issuetype", "priority", "assignee", "reporter", "created", "updated",
    "labels",
];


enum Auth {
    Basic { user: String, token: String },
    Bearer(String),
}

pub struct JiraClient {
    http: Client,
    base: String,
    auth: Auth,
    cloud: bool,
    me: OnceCell<Me>,
}

impl JiraClient {
    pub fn new(cfg: &Config) -> Result<Self> {
        let token = cfg.token()?;
        let auth = match &cfg.jira.email {
            Some(user) => Auth::Basic { user: user.clone(), token },
            None => Auth::Bearer(token),
        };
        let http = Client::builder()
            .user_agent(concat!("jura/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .pool_idle_timeout(Duration::from_secs(90))
            .build()?;
        Ok(Self {
            http,
            base: cfg.jira.url.clone(),
            auth,
            cloud: cfg.jira.deployment == Deployment::Cloud,
            me: OnceCell::new(),
        })
    }

    pub fn browse_url(&self, key: &str) -> String {
        format!("{}/browse/{key}", self.base)
    }

    fn api(&self, path: &str) -> String {
        let version = if self.cloud { "3" } else { "2" };
        format!("{}/rest/api/{version}/{path}", self.base)
    }

    fn req(&self, method: Method, url: String) -> RequestBuilder {
        let rb = self.http.request(method, url).header(header::ACCEPT, "application/json");
        match &self.auth {
            Auth::Basic { user, token } => rb.basic_auth(user, Some(token)),
            Auth::Bearer(token) => rb.bearer_auth(token),
        }
    }

    async fn send(&self, rb: RequestBuilder) -> Result<Value> {
        let resp = rb.send().await?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            bail!("HTTP {status}: {}", error_message(&text));
        }
        if text.trim().is_empty() {
            return Ok(Value::Null);
        }
        Ok(serde_json::from_str(&text)?)
    }

    pub async fn search(&self, jql: &str, max: usize) -> Result<Vec<Issue>> {
        let mut out: Vec<Issue> = Vec::new();
        if self.cloud {
            // Cloud's enhanced search uses cursor pagination.
            let mut cursor: Option<String> = None;
            loop {
                let mut body = json!({
                    "jql": jql,
                    "fields": LIST_FIELDS,
                    "maxResults": (max - out.len()).min(100),
                });
                if let Some(c) = &cursor {
                    body["nextPageToken"] = json!(c);
                }
                let v = self.send(self.req(Method::POST, self.api("search/jql")).json(&body)).await?;
                if let Some(arr) = v["issues"].as_array() {
                    out.extend(arr.iter().map(parse_issue));
                }
                cursor = v["nextPageToken"].as_str().map(String::from);
                if cursor.is_none() || v["isLast"].as_bool() == Some(true) || out.len() >= max {
                    break;
                }
            }
        } else {
            loop {
                let body = json!({
                    "jql": jql,
                    "fields": LIST_FIELDS,
                    "startAt": out.len(),
                    "maxResults": (max - out.len()).min(100),
                });
                let v = self.send(self.req(Method::POST, self.api("search")).json(&body)).await?;
                let page = v["issues"].as_array().map(Vec::as_slice).unwrap_or_default();
                out.extend(page.iter().map(parse_issue));
                let total = v["total"].as_u64().unwrap_or(0) as usize;
                if page.is_empty() || out.len() >= total || out.len() >= max {
                    break;
                }
            }
        }
        out.truncate(max);
        Ok(out)
    }

    pub async fn issue(&self, key: &str) -> Result<IssueDetail> {
        let url = self.api(&format!(
            "issue/{key}?fields={},description,comment&expand=transitions",
            LIST_FIELDS.join(",")
        ));
        let v = self.send(self.req(Method::GET, url)).await?;
        let comments = v["fields"]["comment"]["comments"]
            .as_array()
            .map(|a| a.iter().map(parse_comment).collect())
            .unwrap_or_default();
        Ok(IssueDetail {
            issue: parse_issue(&v),
            description: rich_text(&v["fields"]["description"]),
            comments,
            transitions: parse_transitions(&v),
        })
    }

    pub async fn transitions(&self, key: &str) -> Result<Vec<Transition>> {
        let v = self.send(self.req(Method::GET, self.api(&format!("issue/{key}/transitions")))).await?;
        Ok(parse_transitions(&v))
    }

    pub async fn transition(&self, key: &str, transition_id: &str) -> Result<()> {
        let body = json!({ "transition": { "id": transition_id } });
        let url = self.api(&format!("issue/{key}/transitions"));
        self.send(self.req(Method::POST, url).json(&body)).await?;
        Ok(())
    }

    /// Multi-line text in the format the API expects: Atlassian Document Format
    /// (one paragraph per line) on Cloud v3, plain text on Server/DC v2.
    fn text_body(&self, text: &str) -> Value {
        if !self.cloud {
            return json!(text);
        }
        json!({ "type": "doc", "version": 1, "content": markdown_blocks(text) })
    }

    pub async fn add_comment(&self, key: &str, text: &str) -> Result<()> {
        let body = json!({ "body": self.text_body(text) });
        let url = self.api(&format!("issue/{key}/comment"));
        self.send(self.req(Method::POST, url).json(&body)).await?;
        Ok(())
    }

    /// Names of the issue types a project accepts, excluding sub-tasks.
    pub async fn issue_types(&self, project: &str) -> Result<Vec<String>> {
        let url = self.api(&format!("project/{project}"));
        let v = self.send(self.req(Method::GET, url)).await?;
        let types = v["issueTypes"].as_array().map(Vec::as_slice).unwrap_or_default();
        Ok(types
            .iter()
            .filter(|t| !t["subtask"].as_bool().unwrap_or(false))
            .filter_map(|t| t["name"].as_str().map(String::from))
            .collect())
    }

    /// Creates an issue and returns its key. `issue_type` is a type name such as "Task".
    pub async fn create_issue(
        &self,
        project: &str,
        issue_type: &str,
        summary: &str,
        description: Option<&str>,
    ) -> Result<String> {
        let mut fields = json!({
            "project": { "key": project },
            "issuetype": { "name": issue_type },
            "summary": summary,
        });
        if let Some(d) = description.filter(|d| !d.trim().is_empty()) {
            fields["description"] = self.text_body(d);
        }
        let url = self.api("issue");
        let v = self.send(self.req(Method::POST, url).json(&json!({ "fields": fields }))).await?;
        match v["key"].as_str() {
            Some(key) => Ok(key.to_string()),
            None => bail!("Jira did not return a key for the new issue"),
        }
    }

    pub async fn me(&self) -> Result<Me> {
        let me = self
            .me
            .get_or_try_init(|| async {
                let v = self.send(self.req(Method::GET, self.api("myself"))).await?;
                let id = if self.cloud { &v["accountId"] } else { &v["name"] };
                Ok::<_, anyhow::Error>(Me {
                    id: id.as_str().unwrap_or_default().to_string(),
                    display_name: v["displayName"].as_str().unwrap_or("me").to_string(),
                })
            })
            .await?;
        Ok(me.clone())
    }

    /// Assigns to the given user id, or unassigns when `None`.
    pub async fn assign(&self, key: &str, user_id: Option<&str>) -> Result<()> {
        let body = if self.cloud { json!({ "accountId": user_id }) } else { json!({ "name": user_id }) };
        let url = self.api(&format!("issue/{key}/assignee"));
        self.send(self.req(Method::PUT, url).json(&body)).await?;
        Ok(())
    }
}
