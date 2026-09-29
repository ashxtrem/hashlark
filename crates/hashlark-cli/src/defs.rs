// SPDX-License-Identifier: GPL-3.0-or-later

//! `hashlark-cli defs …`: tools for writing provider definitions.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use clap::{Subcommand, ValueEnum};
use hashlark_core::definition::spec::ResponseKind;
use hashlark_core::definition::templates::{StarterKind, starter};
use hashlark_core::definition::{self, Definition};
use hashlark_core::provider::DefinitionProvider;
use hashlark_core::{ProviderCtx, SearchQuery, SearchResult};
use url::Url;

use crate::format;

#[derive(Debug, Subcommand)]
pub enum DefsCommand {
    /// Create a starter definition.
    New {
        /// Definition id, e.g. `my-site`.
        id: String,
        /// What the site returns.
        #[arg(long = "type", value_enum, default_value = "html")]
        kind: Kind,
        /// Where to write it (default: `<id>.yml`).
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Check definitions for errors.
    Lint {
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// Run a definition against a saved page or the live site.
    Test {
        file: PathBuf,
        /// Parse this saved page instead of fetching (no network).
        #[arg(long, conflicts_with = "live")]
        fixture: Option<PathBuf>,
        /// Fetch from the site.
        #[arg(long)]
        live: bool,
        /// Search text (default: the definition's test query).
        #[arg(short, long)]
        query: Option<String>,
        /// With --live: save the fetched page as a fixture.
        #[arg(long, requires = "live")]
        record: bool,
        /// Where --record saves to (default: fixtures/<id>/search.<ext>).
        #[arg(long)]
        record_to: Option<PathBuf>,
        /// Page URL used to resolve relative links with --fixture.
        #[arg(long)]
        url: Option<Url>,
        /// A setting value (`cfg.name`), repeatable: --set username=ada
        #[arg(long = "set", value_parser = parse_setting)]
        settings: Vec<(String, String)>,
        /// Print every field of every result.
        #[arg(short, long)]
        verbose: bool,
    },
    /// Print the JSON Schema of the definition format (for editors).
    Schema,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Kind {
    Html,
    Json,
    Rss,
}

fn parse_setting(s: &str) -> Result<(String, String), String> {
    s.split_once('=')
        .map(|(k, v)| (k.trim().to_owned(), v.to_owned()))
        .ok_or_else(|| "expected name=value".to_owned())
}

pub async fn run(command: DefsCommand) -> anyhow::Result<()> {
    match command {
        DefsCommand::New { id, kind, output } => new(&id, kind, output),
        DefsCommand::Lint { files } => lint(&files),
        DefsCommand::Test {
            file,
            fixture,
            live,
            query,
            record,
            record_to,
            url,
            settings,
            verbose,
        } => {
            let cfg: BTreeMap<String, String> = settings.into_iter().collect();
            test(TestArgs {
                file,
                fixture,
                live,
                query,
                record,
                record_to,
                url,
                cfg,
                verbose,
            })
            .await
        }
        DefsCommand::Schema => {
            let schema = schemars::schema_for!(Definition);
            println!("{}", serde_json::to_string_pretty(&schema)?);
            Ok(())
        }
    }
}

fn new(id: &str, kind: Kind, output: Option<PathBuf>) -> anyhow::Result<()> {
    let kind = match kind {
        Kind::Html => StarterKind::Html,
        Kind::Json => StarterKind::Json,
        Kind::Rss => StarterKind::Rss,
    };
    let yaml = starter(kind, id);
    definition::load(&yaml).map_err(|e| anyhow::anyhow!("invalid id: {e}"))?;
    let path = output.unwrap_or_else(|| PathBuf::from(format!("{id}.yml")));
    if path.exists() {
        bail!("{} already exists", path.display());
    }
    std::fs::write(&path, yaml).with_context(|| format!("writing {}", path.display()))?;
    println!("Created {}. Next:", path.display());
    println!("  1. Fill in the links, search path, rows and fields.");
    println!("  2. hashlark-cli defs test {} --live", path.display());
    println!(
        "  3. hashlark-cli defs test {} --live --record",
        path.display()
    );
    Ok(())
}

fn lint(files: &[PathBuf]) -> anyhow::Result<()> {
    let mut failed = 0;
    for file in files {
        let yaml =
            std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
        match definition::load(&yaml) {
            Ok(def) => println!(
                "ok    {}  ({} v{})",
                file.display(),
                def.spec.id,
                def.spec.version
            ),
            Err(e) => {
                failed += 1;
                println!("FAIL  {}", file.display());
                for error in &e.errors {
                    println!("      - {error}");
                }
            }
        }
    }
    if failed > 0 {
        bail!("{failed} of {} definition(s) failed", files.len());
    }
    Ok(())
}

struct TestArgs {
    file: PathBuf,
    fixture: Option<PathBuf>,
    live: bool,
    query: Option<String>,
    record: bool,
    record_to: Option<PathBuf>,
    url: Option<Url>,
    cfg: BTreeMap<String, String>,
    verbose: bool,
}

async fn test(args: TestArgs) -> anyhow::Result<()> {
    let yaml = std::fs::read_to_string(&args.file)
        .with_context(|| format!("reading {}", args.file.display()))?;
    let provider = DefinitionProvider::from_yaml(&yaml).map_err(|e| {
        anyhow::anyhow!(
            "{} is invalid:\n  - {}",
            args.file.display(),
            e.errors.join("\n  - ")
        )
    })?;
    let def = provider.definition();
    let text = args
        .query
        .or_else(|| def.spec.test.as_ref().map(|t| t.query.clone()))
        .unwrap_or_else(|| "linux".into());
    let query = SearchQuery::text(&text);

    let (body, page_url) = if let Some(fixture) = &args.fixture {
        let body = std::fs::read_to_string(fixture)
            .with_context(|| format!("reading {}", fixture.display()))?;
        let url = args
            .url
            .clone()
            .or_else(|| def.links.first().cloned())
            .context("the definition has no links")?;
        (body, url)
    } else if args.live {
        let ctx = ProviderCtx {
            config: std::sync::Arc::new(args.cfg.clone()),
            ..ProviderCtx::new()?
        };
        eprintln!("Fetching \"{text}\" from {} …", def.spec.name);
        let (body, url) = provider
            .fetch_page(&ctx, &query)
            .await
            .map_err(|e| anyhow::anyhow!("request failed: {e}"))?;
        eprintln!("Got {} from {url}", format::bytes(body.len() as u64));
        ((*body).clone(), url)
    } else {
        bail!("pass --fixture <file> or --live");
    };

    if args.record {
        let ext = match def.spec.search.response {
            ResponseKind::Html => "html",
            ResponseKind::Json => "json",
            ResponseKind::Xml => "xml",
        };
        let path = args.record_to.unwrap_or_else(|| {
            Path::new("fixtures")
                .join(&def.spec.id)
                .join(format!("search.{ext}"))
        });
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, &body)?;
        eprintln!("Saved the page to {}", path.display());
    }

    let results = provider
        .parse_page(&body, &page_url, &query, &args.cfg)
        .map_err(|e| anyhow::anyhow!("parsing failed: {e}"))?;
    print_results(&results, args.verbose)?;
    if results.is_empty() {
        bail!("no results: check `search.rows` and the `title` field");
    }
    Ok(())
}

fn print_results(results: &[SearchResult], verbose: bool) -> anyhow::Result<()> {
    let mut out = std::io::stdout().lock();
    writeln!(
        out,
        "{:>3}  {:<56}  {:>9}  {:>11}  {:>5}  LINK",
        "#", "TITLE", "SIZE", "SEED/LEECH", "AGE"
    )?;
    for (i, r) in results.iter().enumerate() {
        let link = if r.magnet.is_some() {
            "magnet"
        } else if r.info_hash.is_some() {
            "infohash"
        } else if r.torrent_url.is_some() {
            ".torrent"
        } else if r.needs_resolve {
            "details page"
        } else {
            "-"
        };
        writeln!(
            out,
            "{:>3}  {:<56}  {:>9}  {:>11}  {:>5}  {link}",
            i + 1,
            format::truncate(&r.title, 56),
            r.size_bytes.map_or_else(|| "-".into(), format::bytes),
            format!("{}/{}", format::count(r.seeders), format::count(r.leechers)),
            r.published.map_or_else(|| "-".into(), format::age),
        )?;
        if verbose {
            writeln!(out, "     {}", serde_json::to_string(r)?)?;
        }
    }
    let count = |f: fn(&SearchResult) -> bool| results.iter().filter(|r| f(r)).count();
    writeln!(
        out,
        "\n{} results · size {}/{} · seeders {}/{} · date {}/{} · category {}/{}",
        results.len(),
        count(|r| r.size_bytes.is_some()),
        results.len(),
        count(|r| r.seeders.is_some()),
        results.len(),
        count(|r| r.published.is_some()),
        results.len(),
        count(|r| r.category.is_some()),
        results.len(),
    )?;
    Ok(())
}
