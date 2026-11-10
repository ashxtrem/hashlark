// SPDX-License-Identifier: GPL-3.0-or-later

// Release builds are GUI apps with no console window on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod lan;
mod magnet_handler;
mod open;
mod power;
mod tray;

use std::path::PathBuf;
use std::sync::Arc;

use hashlark_core::engine::EngineOptions;
use hashlark_core::secrets::{KeychainSecretStore, SecretStore};
use hashlark_core::{AppPaths, Engine};
use hashlark_server::{LocalServer, serve_local};
use hashlark_torrent::paths::{desktop_download_dir, server_download_dir};
use hashlark_torrent::{PathPolicy, ServiceOptions, TorrentService};
use tauri::{AppHandle, Manager, RunEvent};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
use tracing_subscriber::EnvFilter;

/// The engine and its in-process API server.
#[derive(Debug)]
pub struct Backend {
    pub engine: Engine,
    pub server: LocalServer,
    pub secrets: Arc<dyn SecretStore>,
    pub data_dir: PathBuf,
    /// The built-in torrent client (plan `docs/torrent-client-plan.md` §6).
    pub torrents: TorrentService,
}

/// Shows, unminimises and focuses the main window.
pub fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

async fn start_backend() -> anyhow::Result<Backend> {
    let paths = AppPaths::resolve(None)?;
    let data_dir = paths.data_dir().to_path_buf();
    let secrets: Arc<dyn SecretStore> = Arc::new(KeychainSecretStore::new("Hashlark"));
    let engine = Engine::open(paths, EngineOptions::new(Arc::clone(&secrets))).await?;
    // For writing definitions: load and live-reload a folder of YAML files.
    if let Some(dir) = std::env::var_os("HASHLARK_DEFINITIONS_DIR") {
        engine.watch_definitions(dir.into()).await?;
    }
    engine.start_background_tasks();
    let download_dir = desktop_download_dir().unwrap_or_else(|| server_download_dir(&data_dir));
    let torrents = TorrentService::new(
        engine.clone(),
        ServiceOptions {
            path_policy: PathPolicy::Unrestricted,
            ..ServiceOptions::new(&data_dir, download_dir)
        },
    );
    if let Err(err) = torrents.start().await {
        tracing::error!(%err, "the torrent client did not start");
    }
    let server = serve_local(engine.clone(), torrents.clone()).await?;
    tracing::info!(api = %server.base_url(), "backend started");
    Ok(Backend {
        engine,
        server,
        secrets,
        data_dir,
        torrents,
    })
}

fn main() {
    // The updater plugin turns on rustls' `ring` backend and reqwest and
    // axum-server turn on `aws-lc-rs`. With both on, rustls can't choose a
    // default and panics at the first TLS connection, so choose one here,
    // before anything connects. `Err` only means one is already installed.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tauri::Builder::default()
        // A second launch focuses the running window instead of starting
        // another engine on the same database, and passes on the magnet or
        // `.torrent` it was started with.
        .plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            show_main_window(app);
            open::handle_args(app, &args, std::path::Path::new(&cwd));
        }))
        // Only for (un)registering as the `magnet:` handler on request: no
        // schemes in the config, so installers don't take over magnet links.
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(
            |app| match tauri::async_runtime::block_on(start_backend()) {
                Ok(backend) => {
                    let lan = lan::Lan::new(
                        app.handle().clone(),
                        Arc::clone(&backend.secrets),
                        &backend.data_dir,
                    );
                    let torrents = backend.torrents.clone();
                    app.manage(backend);
                    app.manage(lan);
                    app.manage(open::Opens::default());
                    app.manage(power::Power::default());
                    if let Err(err) = tray::build(app.handle()) {
                        tracing::error!(%err, "could not create the tray icon");
                    }
                    power::spawn(app.handle(), torrents);
                    // A magnet or `.torrent` this first launch was opened with.
                    let args: Vec<String> = std::env::args().collect();
                    let cwd = std::env::current_dir().unwrap_or_default();
                    open::handle_args(app.handle(), &args, &cwd);
                    // Start LAN sharing (if on) without delaying the window.
                    let handle = app.handle().clone();
                    tauri::async_runtime::spawn(async move {
                        handle.state::<lan::Lan>().start_if_enabled().await;
                    });
                    Ok(())
                }
                Err(e) => {
                    tracing::error!(error = %e, "could not start");
                    app.dialog()
                        .message(format!("Hashlark could not start:\n\n{e:#}"))
                        .title("Hashlark")
                        .kind(MessageDialogKind::Error)
                        .blocking_show();
                    Err(e.into())
                }
            },
        )
        .invoke_handler(tauri::generate_handler![
            commands::api_endpoint,
            commands::open_magnet,
            commands::open_url,
            commands::save_torrent,
            commands::reveal_path,
            commands::magnet_handler_status,
            commands::magnet_handler_register,
            open::take_torrent_opens,
            open::read_torrent_file,
            power::set_when_done,
            power::when_done,
            power::cancel_when_done,
            commands::open_challenge,
            commands::finish_challenge,
            lan::lan_status,
            lan::lan_set_enabled,
            lan::lan_devices,
            lan::lan_open_pairing,
            lan::lan_close_pairing,
            lan::lan_forget,
            lan::lan_add_address,
            lan::lan_pair,
            lan::lan_send,
        ])
        .on_window_event(tray::on_window_event)
        .build(tauri::generate_context!())
        .expect("error while building Hashlark")
        .run(|app, event| match event {
            RunEvent::Exit => {
                // Say goodbye on the network so phones drop this computer at once.
                if let Some(lan) = app.try_state::<lan::Lan>() {
                    tauri::async_runtime::block_on(lan.stop());
                }
                // Flush fastresume and the up/down totals.
                if let Some(backend) = app.try_state::<Backend>() {
                    tauri::async_runtime::block_on(backend.torrents.stop());
                }
            }
            // macOS hands over magnets and `.torrent` files as "open" events.
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            RunEvent::Opened { urls } => {
                let args: Vec<String> = std::iter::once(String::new())
                    .chain(urls.iter().map(|u| u.to_string()))
                    .collect();
                open::handle_args(app, &args, std::path::Path::new("/"));
            }
            _ => {}
        });
}
