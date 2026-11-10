// SPDX-License-Identifier: GPL-3.0-or-later

// Release builds are GUI apps with no console window on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod lan;
mod magnet_handler;

use std::path::PathBuf;
use std::sync::Arc;

use hashlark_core::engine::EngineOptions;
use hashlark_core::secrets::{KeychainSecretStore, SecretStore};
use hashlark_core::{AppPaths, Engine};
use hashlark_server::{LocalServer, serve_local};
use hashlark_torrent::paths::{desktop_download_dir, server_download_dir};
use hashlark_torrent::{PathPolicy, ServiceOptions, TorrentService};
use tauri::{Manager, RunEvent};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
use tracing_subscriber::EnvFilter;

/// The engine and its in-process API server.
#[derive(Debug)]
pub struct Backend {
    pub engine: Engine,
    pub server: LocalServer,
    pub secrets: Arc<dyn SecretStore>,
    pub data_dir: PathBuf,
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
    let server = serve_local(engine.clone(), torrents).await?;
    tracing::info!(api = %server.base_url(), "backend started");
    Ok(Backend {
        engine,
        server,
        secrets,
        data_dir,
    })
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tauri::Builder::default()
        // A second launch focuses the running window instead of starting
        // another engine on the same database.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
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
                    app.manage(backend);
                    app.manage(lan);
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
        .build(tauri::generate_context!())
        .expect("error while building Hashlark")
        .run(|app, event| {
            // Say goodbye on the network so phones drop this computer at once.
            if let RunEvent::Exit = event
                && let Some(lan) = app.try_state::<lan::Lan>()
            {
                tauri::async_runtime::block_on(lan.stop());
            }
        });
}
