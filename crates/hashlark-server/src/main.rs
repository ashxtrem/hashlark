// SPDX-License-Identifier: GPL-3.0-or-later

mod config;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use clap::{Parser, Subcommand};
use hashlark_core::engine::EngineOptions;
use hashlark_core::secrets::{FileSecretStore, KeychainSecretStore, SecretStore};
use hashlark_core::{AppPaths, Engine};
use hashlark_server::{AuthConfig, HostPolicy, ServerConfig, UiSource, router};
use tracing_subscriber::EnvFilter;

use config::{FileConfig, SecretsBackend};

const DEFAULT_BIND: &str = "127.0.0.1:8787";

/// Hashlark headless server: the search engine, its API, the web UI and a
/// Torznab endpoint.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,

    /// Config file (default: <data dir>/hashlark.toml, if present).
    #[arg(long, env = "HASHLARK_CONFIG", global = true)]
    config: Option<PathBuf>,

    /// Address to listen on [default: 127.0.0.1:8787].
    #[arg(long, env = "HASHLARK_BIND")]
    bind: Option<SocketAddr>,

    /// Data directory (default: the OS app-data folder).
    #[arg(long, env = "HASHLARK_DATA_DIR", global = true)]
    data_dir: Option<PathBuf>,

    /// A fixed API token, in addition to API keys (for scripts and development).
    #[arg(long, env = "HASHLARK_TOKEN", hide_env_values = true)]
    token: Option<String>,

    /// Extra browser origin allowed to call the API (repeatable).
    #[arg(
        long = "cors-origin",
        env = "HASHLARK_CORS_ORIGINS",
        value_delimiter = ','
    )]
    cors_origins: Vec<String>,

    /// Load and live-reload provider definitions from this folder.
    #[arg(long, env = "HASHLARK_DEFINITIONS_DIR")]
    definitions_dir: Option<PathBuf>,

    /// TLS certificate chain (PEM). Requires --tls-key.
    #[arg(long, env = "HASHLARK_TLS_CERT", requires = "tls_key")]
    tls_cert: Option<PathBuf>,

    /// TLS private key (PEM).
    #[arg(long, env = "HASHLARK_TLS_KEY", requires = "tls_cert")]
    tls_key: Option<PathBuf>,

    /// Don't serve the web UI.
    #[arg(long)]
    no_web_ui: bool,

    /// Print the OpenAPI document as JSON and exit.
    #[arg(long)]
    openapi: bool,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Manage API keys (works while the server is stopped too).
    Key {
        #[command(subcommand)]
        action: KeyAction,
    },
}

#[derive(Debug, Subcommand)]
enum KeyAction {
    /// Create a key and print it.
    Create { name: String },
    /// List keys.
    List,
    /// Revoke a key by id.
    Revoke { id: String },
}

/// Settings after merging flags, environment and the config file.
struct Resolved {
    file: FileConfig,
    paths: AppPaths,
}

fn resolve(args: &Args) -> anyhow::Result<Resolved> {
    let paths = AppPaths::resolve(args.data_dir.as_deref())?;
    let config_path = args
        .config
        .clone()
        .or_else(|| Some(paths.data_dir().join("hashlark.toml")).filter(|p| p.exists()));
    let file = match &config_path {
        Some(path) => FileConfig::load(path)?,
        None => FileConfig::default(),
    };
    // The config file may move the data directory unless a flag set it.
    let paths = match (&args.data_dir, &file.data_dir) {
        (None, Some(dir)) => AppPaths::at(dir),
        _ => paths,
    };
    Ok(Resolved { file, paths })
}

async fn open_engine(resolved: &Resolved) -> anyhow::Result<Engine> {
    let secrets: Arc<dyn SecretStore> = match resolved.file.secrets.unwrap_or(SecretsBackend::File)
    {
        SecretsBackend::File => Arc::new(FileSecretStore::new(
            resolved.paths.data_dir().join("secrets.json"),
        )),
        SecretsBackend::Keychain => Arc::new(KeychainSecretStore::new("Hashlark")),
    };
    Engine::open(resolved.paths.clone(), EngineOptions::new(secrets))
        .await
        .with_context(|| {
            format!(
                "opening data directory {}",
                resolved.paths.data_dir().display()
            )
        })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    if args.openapi {
        println!("{}", hashlark_server::openapi().to_pretty_json()?);
        return Ok(());
    }
    let resolved = resolve(&args)?;
    let filter = std::env::var("RUST_LOG")
        .ok()
        .or_else(|| resolved.file.log.clone())
        .unwrap_or_else(|| "info".into());
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_new(&filter).unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let engine = open_engine(&resolved).await?;
    if let Some(Command::Key { action }) = args.command {
        return keys(&engine, action).await;
    }
    serve(args, resolved, engine).await
}

async fn keys(engine: &Engine, action: KeyAction) -> anyhow::Result<()> {
    match action {
        KeyAction::Create { name } => {
            let key = engine.create_api_key(&name).await?;
            println!("{}", key.key);
            eprintln!(
                "Created key `{}` ({}). It is shown only once.",
                key.info.name, key.info.id
            );
        }
        KeyAction::List => {
            for key in engine.api_keys().await? {
                println!(
                    "{}  {:<20}  last used: {}",
                    key.id,
                    key.name,
                    key.last_used_at
                        .map_or_else(|| "never".into(), |t| t.to_string())
                );
            }
        }
        KeyAction::Revoke { id } => {
            engine.revoke_api_key(&id).await?;
            eprintln!("Revoked {id}");
        }
    }
    Ok(())
}

async fn serve(args: Args, resolved: Resolved, engine: Engine) -> anyhow::Result<()> {
    let file = &resolved.file;
    let bind = args
        .bind
        .or(file.bind)
        .unwrap_or_else(|| DEFAULT_BIND.parse().expect("valid default"));
    let tls = match (args.tls_cert.clone(), args.tls_key.clone()) {
        (Some(cert), Some(key)) => Some((cert, key)),
        _ => file.tls.clone().map(|t| (t.cert, t.key)),
    };

    // First start: make an admin key so the server can be used at all.
    if args.token.is_none() && engine.api_keys().await?.is_empty() {
        let key = engine.create_api_key("admin").await?;
        eprintln!();
        eprintln!("  Created the first API key (shown only once):");
        eprintln!();
        eprintln!("      {}", key.key);
        eprintln!();
        eprintln!(
            "  Sign in to the web UI with it, and create more keys under Settings → API keys."
        );
        eprintln!("  Lost it? Run: hashlark-server key create <name>");
        eprintln!();
    }

    engine.start_background_tasks();
    if let Some(dir) = args
        .definitions_dir
        .clone()
        .or(file.definitions_dir.clone())
    {
        engine
            .watch_definitions(dir)
            .await
            .context("watching the definitions folder")?;
    }

    let web_ui = if args.no_web_ui || file.web_ui == Some(false) {
        None
    } else if let Some(dir) = file.web_ui_dir.clone() {
        Some(UiSource::Dir(dir))
    } else if UiSource::embedded_available() {
        Some(UiSource::Embedded)
    } else {
        tracing::warn!("this build has no web UI; build apps/desktop first to embed it");
        None
    };

    let mut cors = args.cors_origins.clone();
    cors.extend(file.cors_origins.iter().cloned());
    let config = ServerConfig {
        auth: AuthConfig {
            token: args.token.clone(),
            api_keys: true,
        },
        host_policy: if bind.ip().is_loopback() {
            HostPolicy::Loopback
        } else {
            HostPolicy::Any
        },
        cors_origins: cors
            .iter()
            .map(|o| o.parse())
            .collect::<Result<_, _>>()
            .context("invalid CORS origin")?,
        web_ui,
    };

    if !bind.ip().is_loopback() && tls.is_none() {
        tracing::warn!(
            "listening on {bind} without TLS: API keys travel in clear text. Enable [tls] or put Hashlark behind an HTTPS reverse proxy."
        );
    }
    let app = router(engine, config).into_make_service_with_connect_info::<SocketAddr>();
    let handle = axum_server::Handle::new();
    let shutdown = handle.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("shutting down");
        shutdown.graceful_shutdown(Some(std::time::Duration::from_secs(5)));
    });

    tracing::info!(
        addr = %bind,
        tls = tls.is_some(),
        data_dir = %resolved.paths.data_dir().display(),
        version = hashlark_core::VERSION,
        "hashlark-server listening"
    );
    match tls {
        Some((cert, key)) => {
            let tls = axum_server::tls_rustls::RustlsConfig::from_pem_file(&cert, &key)
                .await
                .with_context(|| format!("loading TLS certificate {}", cert.display()))?;
            axum_server::bind_rustls(bind, tls)
                .handle(handle)
                .serve(app)
                .await?;
        }
        None => {
            axum_server::bind(bind).handle(handle).serve(app).await?;
        }
    }
    Ok(())
}
