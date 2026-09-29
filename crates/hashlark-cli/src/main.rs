// SPDX-License-Identifier: GPL-3.0-or-later

mod defs;
mod format;
mod repo;

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use clap::{Parser, Subcommand};
use hashlark_core::engine::EngineOptions;
use hashlark_core::secrets::KeychainSecretStore;
use hashlark_core::{
    AppPaths, CancellationToken, Category, DownloadTarget, Engine, MergedResult, SearchEvent,
    SearchQuery, SortOrder, Store,
};
use tracing_subscriber::EnvFilter;

/// Hashlark command-line tool: search, manage providers, write definitions.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    /// Data directory (defaults to the OS app-data folder, shared with the
    /// desktop app).
    #[arg(long, global = true, env = "HASHLARK_DATA_DIR")]
    data_dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Search all enabled providers.
    Search(SearchArgs),
    /// List providers with their health.
    Providers,
    /// Add a provider from a YAML definition file.
    Import { file: PathBuf },
    /// Tools for writing provider definitions.
    #[command(subcommand)]
    Defs(defs::DefsCommand),
    /// Publish and use signed definition repositories.
    #[command(subcommand)]
    Repo(repo::RepoCommand),
    /// Show version, data locations and database schema version.
    Info,
}

#[derive(Debug, clap::Args)]
struct SearchArgs {
    /// What to search for.
    #[arg(required = true, num_args = 1..)]
    query: Vec<String>,

    /// Limit to a category (repeatable): movies, tv, music, books, software, games, anime, other.
    #[arg(short, long = "category")]
    categories: Vec<Category>,

    /// Only search these providers (repeatable), e.g. internet-archive.
    #[arg(short = 'P', long = "provider")]
    providers: Vec<String>,

    /// Sort order: relevance, title, seeders, peers, size or date.
    #[arg(short, long, default_value = "relevance")]
    sort: SortOrder,

    /// Maximum results to print.
    #[arg(short = 'n', long, default_value_t = 20)]
    limit: usize,

    /// Result page to request from providers.
    #[arg(short, long, default_value_t = 1)]
    page: u32,

    /// Also print each result's magnet link or .torrent URL.
    #[arg(short, long)]
    links: bool,

    /// Print the full outcome as JSON instead of a table.
    #[arg(long)]
    json: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let paths = AppPaths::resolve(cli.data_dir.as_deref())?;
    match cli.command {
        Command::Search(args) => search(&open(&paths).await?, args).await,
        Command::Providers => providers(&open(&paths).await?).await,
        Command::Import { file } => import(&open(&paths).await?, &file).await,
        Command::Defs(command) => defs::run(command).await,
        Command::Repo(command) => repo::run(command, async || open(&paths).await).await,
        Command::Info => info(&paths).await,
    }
}

async fn open(paths: &AppPaths) -> anyhow::Result<Engine> {
    let secrets = Arc::new(KeychainSecretStore::new("Hashlark"));
    Engine::open(paths.clone(), EngineOptions::new(secrets))
        .await
        .with_context(|| format!("opening data directory {}", paths.data_dir().display()))
}

async fn search(engine: &Engine, args: SearchArgs) -> anyhow::Result<()> {
    let query = SearchQuery {
        categories: args.categories,
        providers: (!args.providers.is_empty())
            .then(|| args.providers.iter().map(|p| p.as_str().into()).collect()),
        page: args.page,
        sort: args.sort,
        ..SearchQuery::text(args.query.join(" "))
    };

    if args.json {
        let outcome = engine.search_all(query).await?;
        serde_json::to_writer_pretty(std::io::stdout().lock(), &outcome)?;
        println!();
        return Ok(());
    }

    // Stream provider progress to stderr while collecting results.
    let mut rx = engine.search(query, CancellationToken::new()).await?;
    let mut results: Vec<MergedResult> = Vec::new();
    let mut searched = 0;
    let mut took_ms = 0;
    while let Some(event) = rx.recv().await {
        match event {
            SearchEvent::ProviderStarted { .. } => searched += 1,
            SearchEvent::Results { items, .. } => {
                for item in items {
                    match results.iter_mut().find(|r| r.id == item.id) {
                        Some(existing) => *existing = item,
                        None => results.push(item),
                    }
                }
            }
            SearchEvent::ProviderFinished {
                provider,
                count,
                latency_ms,
            } => eprintln!(
                "  ok   {provider:<20} {count:>4} results  {}",
                format::duration_ms(latency_ms)
            ),
            SearchEvent::ProviderFailed {
                provider,
                error_kind,
                message,
                latency_ms,
            } => eprintln!(
                "  FAIL {provider:<20} {:<16} {message} ({})",
                error_kind.as_str(),
                format::duration_ms(latency_ms)
            ),
            SearchEvent::Done { duration_ms, .. } => took_ms = duration_ms,
        }
    }

    if searched == 0 {
        anyhow::bail!("no enabled provider matches this query (check --category / --provider)");
    }

    hashlark_core::ranking::sort(&mut results, args.sort);
    eprintln!(
        "\n{} results from {searched} provider(s) in {}\n",
        results.len(),
        format::duration_ms(took_ms)
    );
    print_table(
        engine,
        &results[..results.len().min(args.limit)],
        args.links,
    )
    .await
}

async fn print_table(engine: &Engine, results: &[MergedResult], links: bool) -> anyhow::Result<()> {
    let mut out = std::io::stdout().lock();
    writeln!(
        out,
        "{:>3}  {:<60}  {:>9}  {:>11}  {:>5}  SOURCES",
        "#", "TITLE", "SIZE", "SEED/LEECH", "AGE"
    )?;
    for (i, result) in results.iter().enumerate() {
        let p = &result.primary;
        writeln!(
            out,
            "{:>3}  {:<60}  {:>9}  {:>11}  {:>5}  {}",
            i + 1,
            format::truncate(&p.title, 60),
            p.size_bytes.map_or_else(|| "-".into(), format::bytes),
            format!(
                "{}/{}",
                format::count(result.seeders),
                format::count(result.leechers)
            ),
            p.published.map_or_else(|| "-".into(), format::age),
            result
                .sources
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(","),
        )?;
        if links {
            let link = match engine.resolve(&result.id, None).await {
                Ok(DownloadTarget::Magnet(m)) => m,
                Ok(DownloadTarget::TorrentFile(url)) => url.to_string(),
                Err(err) => format!("(could not resolve: {err})"),
            };
            writeln!(out, "     {link}")?;
        }
    }
    Ok(())
}

async fn providers(engine: &Engine) -> anyhow::Result<()> {
    println!(
        "{:<20}  {:<20}  {:<10}  {:<8}  {:<13}  CATEGORIES",
        "ID", "NAME", "KIND", "ENABLED", "HEALTH"
    );
    for p in engine.providers().await? {
        let health = serde_json::to_value(p.health.state)?
            .as_str()
            .unwrap_or_default()
            .to_owned();
        println!(
            "{:<20}  {:<20}  {:<10}  {:<8}  {:<13}  {}",
            p.id,
            format::truncate(&p.name, 20),
            serde_json::to_value(p.kind)?.as_str().unwrap_or_default(),
            if p.enabled { "yes" } else { "no" },
            health,
            if p.categories.is_empty() {
                "any".to_owned()
            } else {
                p.categories
                    .iter()
                    .map(|c| c.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            }
        );
        if let Some(error) = p.error {
            println!("{:>22}error: {error}", "");
        }
    }
    Ok(())
}

async fn import(engine: &Engine, file: &PathBuf) -> anyhow::Result<()> {
    let yaml =
        std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
    match engine.add_definition(&yaml).await {
        Ok(view) => {
            println!("Added provider `{}` ({})", view.id, view.name);
            Ok(())
        }
        Err(hashlark_core::Error::Definition(e)) => {
            anyhow::bail!(
                "the definition is invalid:\n  - {}",
                e.errors.join("\n  - ")
            )
        }
        Err(e) => Err(e.into()),
    }
}

async fn info(paths: &AppPaths) -> anyhow::Result<()> {
    paths.ensure_dirs()?;
    let store = Store::open(&paths.db_path())
        .await
        .with_context(|| format!("opening database at {}", paths.db_path().display()))?;
    let schema = store.schema_version().await?;

    println!("hashlark     {}", hashlark_core::VERSION);
    println!("data dir     {}", paths.data_dir().display());
    println!("database     {}", paths.db_path().display());
    println!(
        "schema       {}",
        schema.map_or_else(|| "none".to_owned(), |v| v.to_string())
    );
    Ok(())
}
