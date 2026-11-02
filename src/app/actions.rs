use chrono::Utc;

use super::*;
use crate::jira::{Comment, Issue, Transition};

impl App {
    pub(super) fn patch_issue(&mut self, key: &str, patch: impl Fn(&mut Issue)) {
        for f in &mut self.filters {
            f.issues.iter_mut().filter(|i| i.key == key).for_each(&patch);
        }
        if let Some(d) = self.details.get_mut(key) {
            patch(&mut d.data.issue);
        }
    }

    pub(super) fn apply_transition(&mut self, key: String, t: Transition) {
        let (to, cat) = (t.to.clone(), t.to_category.clone());
        self.patch_issue(&key, |i| {
            i.status = to.clone();
            i.status_category = cat.clone();
        });
        self.transitions.remove(&key);
        if let Some(d) = self.details.get_mut(&key) {
            d.data.transitions.clear();
        }
        let client = self.client.clone();
        let label = format!("{key} → {}", self.cfg.status_label(&t.to));
        self.spawn(async move {
            let result = client.transition(&key, &t.id).await.map_err(err_str);
            Msg::Mutation { key, label, result }
        });
    }

    pub(super) fn assign(&mut self, key: String, to_me: bool) {
        let name = if to_me { Some(self.me.as_ref().map_or("me".into(), |m| m.display_name.clone())) } else { None };
        self.patch_issue(&key, |i| i.assignee = name.clone());
        let client = self.client.clone();
        let label = if to_me { format!("{key} assigned to you") } else { format!("{key} unassigned") };
        self.spawn(async move {
            let result = async {
                let id = if to_me { Some(client.me().await?.id) } else { None };
                client.assign(&key, id.as_deref()).await
            }
            .await
            .map_err(err_str);
            Msg::Mutation { key, label, result }
        });
    }

    pub(super) fn create_issue(&mut self, summary: String, issue_type: String) {
        let client = self.client.clone();
        let project = self.cfg.project.clone();
        self.spawn(async move {
            let result = client.create_issue(&project, &issue_type, &summary, None).await;
            Msg::Created(result.map_err(err_str))
        });
    }

    pub(super) fn add_comment(&mut self, key: String, text: String) {
        let author = self.me.as_ref().map_or("You".into(), |m| m.display_name.clone());
        if let Some(d) = self.details.get_mut(&key) {
            d.data.comments.push(Comment { author, created: Utc::now().to_rfc3339(), body: text.clone(), pending: true });
        }
        let client = self.client.clone();
        let label = format!("Comment added to {key}");
        self.spawn(async move {
            let result = client.add_comment(&key, &text).await.map_err(err_str);
            Msg::Mutation { key, label, result }
        });
    }
}
