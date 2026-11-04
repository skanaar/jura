mod app;
mod cache;
mod config;
mod jira;
mod ui;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, bail};
use crossterm::event::{Event, EventStream, KeyEventKind};
use futures::StreamExt;
use ratatui::DefaultTerminal;
use tokio::sync::mpsc;

use app::{App, Msg};

const USAGE: &str = "\
jura - Jira issues in your terminal

Usage: jura [--config <path>]
       jura --init [--config <path>]   write an example config

Config lookup: --config, $JURA_CONFIG, ./jura.toml, then the platform config dir.";

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mut explicit: Option<PathBuf> = None;
    let mut init = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-c" | "--config" => match args.next() {
                Some(p) => explicit = Some(PathBuf::from(p)),
                None => bail!("--config needs a path"),
            },
            "--init" => init = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            other => bail!("unknown argument `{other}`\n\n{USAGE}"),
        }
    }

    if init {
        let path = explicit.unwrap_or_else(config::default_path);
        if path.exists() {
            bail!("{} already exists", path.display());
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, config::EXAMPLE)?;
        println!("Wrote example config to {}", path.display());
        return Ok(());
    }

    let path = config::resolve_path(explicit);
    if !path.exists() {
        bail!(
            "no config found at {}\nRun `jura --init` to create one, or pass --config <path>.",
            path.display()
        );
    }
    let cfg = config::Config::load(&path)?;
    let client = Arc::new(jira::JiraClient::new(&cfg)?);
    let cache = Arc::new(cache::Cache::new(&cfg.cache_scope())?);

    let (tx, rx) = mpsc::unbounded_channel();
    // Cached data is loaded synchronously here, so the very first frame has content.
    let mut app = App::new(cfg, client, cache, tx);
    app.start();

    let mut terminal = ratatui::init();
    crossterm::execute!(std::io::stdout(), crossterm::terminal::SetTitle("🦕 Jura"))?;
    let result = run(&mut terminal, &mut app, rx).await;
    ratatui::restore();
    result
}

async fn run(terminal: &mut DefaultTerminal, app: &mut App, mut rx: mpsc::UnboundedReceiver<Msg>) -> Result<()> {
    let mut events = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    while !app.quit {
        terminal.draw(|f| ui::draw(f, app))?;
        tokio::select! {
            ev = events.next() => match ev {
                // Windows reports both press and release; act on presses only.
                Some(Ok(Event::Key(key))) if key.kind == KeyEventKind::Press => app.on_key(key),
                Some(Ok(_)) => {}
                Some(Err(e)) => return Err(e.into()),
                None => break,
            },
            Some(msg) = rx.recv() => {
                app.on_msg(msg);
                while let Ok(msg) = rx.try_recv() {
                    app.on_msg(msg);
                }
            }
            _ = tick.tick() => app.on_tick(),
        }
    }
    Ok(())
}
