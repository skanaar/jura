use super::*;

impl App {

    pub fn on_msg(&mut self, msg: Msg) {
        match msg {
            Msg::Filter { idx, result } => {
                let f = &mut self.filters[idx];
                f.loading = false;
                match result {
                    Ok(issues) => {
                        let now = Utc::now();
                        self.cache.store(&Cache::filter_name(&f.jql), &Cached { fetched_at: now, data: &issues });
                        f.issues = issues;
                        f.fetched_at = Some(now);
                        f.error = None;
                        if idx == self.active {
                            self.sync_selection();
                        }
                    }
                    Err(e) => {
                        let text = format!("{}: {e}", f.name);
                        f.error = Some(e);
                        self.set_flash(text, true);
                    }
                }
            }
            Msg::Detail { key, result } => {
                self.details_loading.remove(&key);
                match result {
                    Ok(detail) => {
                        let now = Utc::now();
                        self.cache.store(&Cache::issue_name(&key), &Cached { fetched_at: now, data: &detail });
                        // The single-issue fetch is newer than any list; keep lists in sync.
                        let fresh = detail.issue.clone();
                        self.patch_issue(&key, |i| *i = fresh.clone());
                        self.transitions.insert(key.clone(), detail.transitions.clone());
                        self.detail_errors.remove(&key);
                        self.details.insert(key, DetailEntry { data: *detail, fetched_at: now });
                    }
                    Err(e) => {
                        self.detail_errors.insert(key, e);
                    }
                }
            }
            Msg::Transitions { key, result } => {
                self.transitions_loading.remove(&key);
                match result {
                    Ok(list) => {
                        self.transitions.insert(key, list);
                    }
                    Err(e) => self.set_flash(format!("Transitions for {key}: {e}"), true),
                }
            }
            Msg::Me(result) => match result {
                Ok(me) => self.me = Some(me),
                Err(e) => self.set_flash(format!("Could not load current user: {e}"), true),
            },
            Msg::IssueTypes(result) => {
                self.issue_types_loading = false;
                match result {
                    Ok(types) => self.issue_types = Some(types),
                    Err(e) => self.set_flash(format!("Could not load issue types: {e}"), true),
                }
            }
            Msg::Created(result) => {
                match result {
                    Ok(key) => {
                        self.set_flash(format!("✓ Created {key}"), false);
                        self.refresh_all();
                    }
                    Err(e) => self.set_flash(format!("✗ Could not create issue: {e}"), true),
                }
            }
            Msg::Mutation { key, label, result } => {
                match result {
                    Ok(()) => self.set_flash(format!("✓ {label}"), false),
                    Err(e) => self.set_flash(format!("✗ {label}: {e}"), true),
                }
                // Either confirm the optimistic state or roll it back to the truth.
                self.refresh_all();
                self.ensure_detail(&key, true);
            }
        }
    }

    pub fn on_tick(&mut self) {
        self.spinner = self.spinner.wrapping_add(1);
        if let Some((key, at)) = &self.pending_detail
            && at.elapsed() >= DETAIL_DEBOUNCE
        {
            let key = key.clone();
            self.pending_detail = None;
            self.ensure_detail(&key, false);
        }
        if self.last_auto_refresh.elapsed() >= Duration::from_secs(self.cfg.refresh_interval_secs) {
            self.refresh_all();
        }
        if self.flash.as_ref().is_some_and(|f| f.at.elapsed() >= FLASH_TTL) {
            self.flash = None;
        }
    }
}
