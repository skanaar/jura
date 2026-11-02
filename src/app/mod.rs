//! Application state and the stale-while-revalidate logic.
//!
//! Every view renders from local state only. Network work runs in spawned
//! tasks that report back through `Msg`; cached data stays on screen (marked
//! as refreshing) until the fresh result replaces it.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use ratatui::widgets::{ListState, TableState};
use tokio::sync::mpsc::UnboundedSender;

use crate::cache::{Cache, Cached};
use crate::config::Config;
use crate::jira::{Issue, IssueDetail, JiraClient, Me, Transition};

/// Wait this long after the cursor stops before revalidating the issue under it.
const DETAIL_DEBOUNCE: Duration = Duration::from_millis(150);
const FLASH_TTL: Duration = Duration::from_secs(6);

mod actions;
mod events;
mod keys;
mod selection;

pub enum Msg {
    Filter { idx: usize, result: Result<Vec<Issue>, String> },
    Detail { key: String, result: Result<Box<IssueDetail>, String> },
    Transitions { key: String, result: Result<Vec<Transition>, String> },
    Me(Result<Me, String>),
    Mutation { key: String, label: String, result: Result<(), String> },
    Created(Result<String, String>),
    IssueTypes(Result<Vec<String>, String>),
}

pub struct FilterView {
    pub name: String,
    pub jql: String,
    pub issues: Vec<Issue>,
    pub fetched_at: Option<DateTime<Utc>>,
    pub loading: bool,
    pub error: Option<String>,
    pub selected_key: Option<String>,
    pub table: TableState,
}

pub struct DetailEntry {
    pub data: IssueDetail,
    pub fetched_at: DateTime<Utc>,
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum Focus {
    List,
    Detail,
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum DetailTab {
    Description,
    Info,
    Comments,
}

impl DetailTab {
    pub const ALL: [DetailTab; 3] = [DetailTab::Description, DetailTab::Info, DetailTab::Comments];

    fn shifted(self, delta: isize) -> Self {
        let i = Self::ALL.iter().position(|t| *t == self).unwrap_or(0) as isize;
        Self::ALL[(i + delta).clamp(0, Self::ALL.len() as isize - 1) as usize]
    }
}

pub enum Mode {
    Normal,
    Search,
    Transition { key: String, list: ListState },
    Comment { key: String, text: String },
    Create { summary: String, list: ListState },
    Help,
}

pub struct Flash {
    pub text: String,
    pub error: bool,
    at: Instant,
}

pub struct App {
    pub cfg: Config,
    client: Arc<JiraClient>,
    cache: Arc<Cache>,
    tx: UnboundedSender<Msg>,

    pub filters: Vec<FilterView>,
    pub active: usize,
    pub search: String,
    pub mode: Mode,
    pub focus: Focus,
    pub show_preview: bool,
    pub detail_scroll: u16,
    pub detail_tab: DetailTab,

    pub details: HashMap<String, DetailEntry>,
    pub details_loading: HashSet<String>,
    pub detail_errors: HashMap<String, String>,
    pub transitions: HashMap<String, Vec<Transition>>,
    pub transitions_loading: HashSet<String>,
    pub me: Option<Me>,
    /// Issue types of the configured project, loaded the first time they are needed.
    pub issue_types: Option<Vec<String>>,
    pub issue_types_loading: bool,

    pending_detail: Option<(String, Instant)>,
    last_auto_refresh: Instant,
    flash: Option<Flash>,
    pub spinner: usize,
    pub quit: bool,
}

fn err_str(e: anyhow::Error) -> String {
    format!("{e:#}")
}

impl App {
    pub fn new(cfg: Config, client: Arc<JiraClient>, cache: Arc<Cache>, tx: UnboundedSender<Msg>) -> Self {
        let filters = cfg
            .filters
            .iter()
            .map(|f| {
                let cached: Option<Cached<Vec<Issue>>> = cache.load(&Cache::filter_name(&f.jql));
                let (issues, fetched_at) = match cached {
                    Some(c) => (c.data, Some(c.fetched_at)),
                    None => (Vec::new(), None),
                };
                FilterView {
                    name: f.name.clone(),
                    jql: f.jql.clone(),
                    selected_key: issues.first().map(|i| i.key.clone()),
                    issues,
                    fetched_at,
                    loading: false,
                    error: None,
                    table: TableState::default(),
                }
            })
            .collect();
        Self {
            cfg,
            client,
            cache,
            tx,
            filters,
            active: 0,
            search: String::new(),
            mode: Mode::Normal,
            focus: Focus::List,
            show_preview: true,
            detail_scroll: 0,
            detail_tab: DetailTab::Description,
            details: HashMap::new(),
            details_loading: HashSet::new(),
            detail_errors: HashMap::new(),
            transitions: HashMap::new(),
            transitions_loading: HashSet::new(),
            me: None,
            issue_types: None,
            issue_types_loading: false,
            pending_detail: None,
            last_auto_refresh: Instant::now(),
            flash: None,
            spinner: 0,
            quit: false,
        }
    }

    /// Kicks off revalidation of everything. Cached data is already in place.
    pub fn start(&mut self) {
        for idx in 0..self.filters.len() {
            self.refresh_filter(idx);
        }
        let client = self.client.clone();
        self.spawn(async move { Msg::Me(client.me().await.map_err(err_str)) });
        self.on_selection_changed();
    }

    fn spawn<F>(&self, fut: F)
    where
        F: Future<Output = Msg> + Send + 'static,
    {
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let _ = tx.send(fut.await);
        });
    }

    fn is_stale(&self, fetched_at: Option<DateTime<Utc>>) -> bool {
        match fetched_at {
            Some(t) => (Utc::now() - t).num_seconds() >= self.cfg.stale_after_secs as i64,
            None => true,
        }
    }

    pub fn is_busy(&self) -> bool {
        self.filters.iter().any(|f| f.loading) || !self.details_loading.is_empty()
    }

    pub fn flash(&self) -> Option<&Flash> {
        self.flash.as_ref()
    }

    fn set_flash(&mut self, text: impl Into<String>, error: bool) {
        self.flash = Some(Flash { text: text.into(), error, at: Instant::now() });
    }

    // ---- fetching -------------------------------------------------------

    fn refresh_filter(&mut self, idx: usize) {
        let f = &mut self.filters[idx];
        if f.loading {
            return;
        }
        f.loading = true;
        let client = self.client.clone();
        let jql = f.jql.clone();
        let max = self.cfg.max_results;
        self.spawn(async move {
            Msg::Filter { idx, result: client.search(&jql, max).await.map_err(err_str) }
        });
    }

    fn refresh_all(&mut self) {
        for idx in 0..self.filters.len() {
            self.refresh_filter(idx);
        }
        self.last_auto_refresh = Instant::now();
    }

    /// Shows whatever is cached for `key` immediately and revalidates it if stale.
    fn ensure_detail(&mut self, key: &str, force: bool) {
        self.load_detail_from_disk(key);
        let fresh = !self.is_stale(self.details.get(key).map(|d| d.fetched_at));
        if (force || !fresh) && self.details_loading.insert(key.to_string()) {
            let client = self.client.clone();
            let key = key.to_string();
            self.spawn(async move {
                let result = client.issue(&key).await.map(Box::new).map_err(err_str);
                Msg::Detail { key, result }
            });
        }
    }

    fn load_detail_from_disk(&mut self, key: &str) {
        if self.details.contains_key(key) {
            return;
        }
        if let Some(c) = self.cache.load::<IssueDetail>(&Cache::issue_name(key)) {
            self.details.insert(key.to_string(), DetailEntry { data: c.data, fetched_at: c.fetched_at });
        }
    }

    fn fetch_issue_types(&mut self) {
        if self.issue_types.is_none() && !self.issue_types_loading {
            self.issue_types_loading = true;
            let client = self.client.clone();
            let project = self.cfg.project.clone();
            self.spawn(async move { Msg::IssueTypes(client.issue_types(&project).await.map_err(err_str)) });
        }
    }

    fn fetch_transitions(&mut self, key: &str) {
        if self.transitions_loading.insert(key.to_string()) {
            let client = self.client.clone();
            let key = key.to_string();
            self.spawn(async move {
                let result = client.transitions(&key).await.map_err(err_str);
                Msg::Transitions { key, result }
            });
        }
    }
}
