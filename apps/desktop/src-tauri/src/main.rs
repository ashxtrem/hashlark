// SPDX-License-Identifier: GPL-3.0-or-later

// Release builds are GUI apps with no console window on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod magnet_handler;

use std::sync::Arc;

use hashlark_core::engine::EngineOptions;
use hashlark_core::secrets::{KeychainSecretStore, SecretStore};
use hashlark_core::{AppPaths, Engine};
use hashlark_server::{LocalServer, serve_local};
use tauri::Manager;
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
use tracing_subscriber::EnvFilter;

/// The engine and its in-process API server.
#[derive(Debug)]
pub struct Backend {
    pub engine: Engine,
    pub server: LocalServer,
}

async fn start_backend() -> anyhow::Result<Backend> {
    let paths = AppPaths::resolve(None)?;
    let secrets: Arc<dyn SecretStore> = Arc::new(KeychainSecretStore::new("Hashlark"));
    let engine = Engine::open(paths, EngineOptions::new(secrets)).await?;
    // For writing definitions: load and live-reload a folder of YAML files.
    if let Some(dir) = std::env::var_os("HASHLARK_DEFINITIONS_DIR") {
        engine.watch_definitions(dir.into()).await?;
    }
    engine.start_background_tasks();
    let server = serve_local(engine.clone()).await?;
    tracing::info!(api = %server.base_url(), "backend started");
    Ok(Backend { engine, server })
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
                    app.manage(backend);
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running Hashlark");
}
