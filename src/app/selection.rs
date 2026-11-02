use super::*;

impl App {

    pub fn active_filter(&self) -> &FilterView {
        &self.filters[self.active]
    }

    pub fn active_filter_mut(&mut self) -> &mut FilterView {
        &mut self.filters[self.active]
    }

    pub fn matches_search(&self, issue: &Issue) -> bool {
        if self.search.is_empty() {
            return true;
        }
        let q = self.search.to_lowercase();
        let label = self.cfg.status_label(&issue.status);
        [Some(issue.key.as_str()), Some(&issue.summary), Some(&issue.status), Some(label), issue.assignee.as_deref()]
            .into_iter()
            .flatten()
            .any(|s| s.to_lowercase().contains(&q))
    }

    pub fn visible(&self) -> Vec<&Issue> {
        self.active_filter().issues.iter().filter(|i| self.matches_search(i)).collect()
    }

    /// Index of the selection within `visible()`; falls back to the first row.
    pub fn selected_pos(&self) -> Option<usize> {
        let visible = self.visible();
        if visible.is_empty() {
            return None;
        }
        let key = self.active_filter().selected_key.as_deref();
        Some(visible.iter().position(|i| Some(i.key.as_str()) == key).unwrap_or(0))
    }

    pub fn selected_issue(&self) -> Option<&Issue> {
        let pos = self.selected_pos()?;
        self.visible().get(pos).copied()
    }

    pub(super) fn selected_key(&self) -> Option<String> {
        self.selected_issue().map(|i| i.key.clone())
    }

    pub(super) fn move_selection(&mut self, delta: isize) {
        let len = self.visible().len();
        let Some(pos) = self.selected_pos() else { return };
        let new = (pos as isize + delta).clamp(0, len as isize - 1) as usize;
        let key = self.visible()[new].key.clone();
        if self.active_filter().selected_key.as_deref() != Some(&key) || new != pos {
            self.active_filter_mut().selected_key = Some(key);
            self.on_selection_changed();
        }
    }

    /// Pins the effective selection to a concrete key after the list changed.
    pub(super) fn sync_selection(&mut self) {
        let effective = self.selected_key();
        if self.active_filter().selected_key != effective {
            self.active_filter_mut().selected_key = effective;
            self.on_selection_changed();
        }
    }

    pub(super) fn on_selection_changed(&mut self) {
        self.detail_scroll = 0;
        if let Some(key) = self.selected_key() {
            // Disk cache is shown instantly; the network fetch waits until the cursor rests.
            self.load_detail_from_disk(&key);
            self.pending_detail = Some((key, Instant::now()));
        }
    }

    pub(super) fn switch_filter(&mut self, idx: usize) {
        if idx >= self.filters.len() || idx == self.active {
            return;
        }
        self.active = idx;
        self.sync_selection();
        self.on_selection_changed();
        if self.is_stale(self.filters[idx].fetched_at) {
            self.refresh_filter(idx);
        }
    }
}
