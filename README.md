# jura

A fast terminal client for Jira, built with Rust and [Ratatui](https://ratatui.rs).

- One tab per JQL filter from the config file
- Stale-while-revalidate everywhere: the last fetched data is shown instantly
  (also across restarts, from an on-disk cache) while fresh data loads in the
  background. A spinner marks what is being refreshed; there are no loading screens
  after the very first fetch.
- Issue details (description, comments) with a live preview pane
- Create issues, transition, comment, assign to me / unassign, open in browser
- Optimistic updates: changes show immediately and are reconciled with the server

## Setup

```bash
cargo install --path .
```

```bash
jura --init
```

`--init` writes [config.example.toml](config.example.toml) to the platform config
dir (Windows: `%APPDATA%\jura\config\config.toml`). Fill in the site URL, your e-mail
and an [API token](https://id.atlassian.com/manage-profile/security/api-tokens).
The token can also come from an environment variable (`api_token_env`, default
`JIRA_API_TOKEN`) instead of the file.

Statuses can be shown under your own names with a `[status_names]` table
(`"In Progress" = "WIP"`); the custom names also work in search.

Config lookup order: `--config <path>`, `$JURA_CONFIG`, `./jura.toml`, platform config dir.

Jira Server / Data Center is supported with `deployment = "server"` (REST API v2);
omit `email` to authenticate with a Personal Access Token.

## Keys

| Key | Action |
| --- | --- |
| `j` / `k`, arrows | Move (or scroll details) |
| `g` / `G` | First / last issue |
| `Tab` / `Shift+Tab`, `h` / `l`, `1`–`9` | Switch filter |
| `Enter` | Focus details / back |
| `/` | Search within the current filter |
| `t` | Transition |
| `c` | Comment (`Enter` sends, `Alt+Enter` newline) |
| `a` / `A` | Assign to me / unassign |
| `o` | Open in browser |
| `p` | Toggle preview pane |
| `r` / `R` | Refresh filter / all filters |
| `?` | Help |
| `q` | Quit |

## How caching works

| Data | Shown from | Revalidated when |
| --- | --- | --- |
| Filter results | Disk cache, loaded before the first frame | On start, on switching to a stale tab, every `refresh_interval_secs`, after any change |
| Issue details + transitions | Memory, then disk cache | 150 ms after the cursor rests on a stale issue, after any change to it |

"Stale" means older than `stale_after_secs`. Cache files live in the platform
cache dir, scoped per site + user.
