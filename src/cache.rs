//! On-disk cache backing the stale-while-revalidate behaviour: whatever was
//! last fetched is shown instantly on the next start while fresh data loads.

use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

#[derive(Serialize, Deserialize)]
pub struct Cached<T> {
    pub fetched_at: DateTime<Utc>,
    pub data: T,
}

pub struct Cache {
    dir: PathBuf,
}

impl Cache {
    pub fn new(scope: &str) -> Result<Self> {
        let base = directories::ProjectDirs::from("", "", "jura")
            .map(|d| d.cache_dir().to_path_buf())
            .unwrap_or_else(|| std::env::temp_dir().join("jura"));
        let dir = base.join(format!("{:016x}", fnv1a(scope)));
        std::fs::create_dir_all(&dir).with_context(|| format!("creating cache dir {}", dir.display()))?;
        Ok(Self { dir })
    }

    pub fn filter_name(jql: &str) -> String {
        format!("filter-{:016x}.json", fnv1a(jql))
    }

    pub fn issue_name(key: &str) -> String {
        let safe: String = key.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
        format!("issue-{safe}.json")
    }

    /// Synchronous on purpose: cache files are small and reading them before
    /// the first frame is what makes startup feel instant.
    pub fn load<T: DeserializeOwned>(&self, name: &str) -> Option<Cached<T>> {
        let bytes = std::fs::read(self.dir.join(name)).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Serializes now, writes in the background via a temp file + rename so a
    /// crash mid-write never leaves a corrupt entry.
    pub fn store<T: Serialize>(&self, name: &str, value: &Cached<T>) {
        let Ok(bytes) = serde_json::to_vec(value) else { return };
        let path = self.dir.join(name);
        let tmp = self.dir.join(format!("{name}.tmp"));
        tokio::spawn(async move {
            if tokio::fs::write(&tmp, bytes).await.is_ok() {
                let _ = tokio::fs::rename(&tmp, &path).await;
            }
        });
    }
}

/// Stable across Rust versions, unlike `DefaultHasher`.
fn fnv1a(s: &str) -> u64 {
    s.bytes().fold(0xcbf29ce484222325, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}
