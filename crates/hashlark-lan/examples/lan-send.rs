// SPDX-License-Identifier: GPL-3.0-or-later

//! Plays the phone's part from a terminal, for trying the desktop receiver:
//!
//! ```text
//! cargo run -p hashlark-lan --example lan-send -- <ip:port> <pin|-> <magnet> [title]
//! ```
//!
//! The first run needs the PIN the desktop shows under Settings → Devices on
//! this network → Pair a phone. The pairing is kept in a file in the temp
//! folder, so later runs can pass `-` instead of a PIN.

use std::sync::Arc;

use hashlark_core::secrets::{FileSecretStore, SecretStore};
use hashlark_lan::{LanConfig, LanEvents, LanService, Platform, Received};

struct Print;

impl LanEvents for Print {
    fn received(&self, item: Received) {
        println!("received “{}” from {}", item.title, item.from.name);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [addr, pin, magnet, rest @ ..] = args.as_slice() else {
        eprintln!("usage: lan-send <ip:port> <pin|-> <magnet> [title]");
        std::process::exit(2);
    };
    let title = rest.first().map_or("Sent by lan-send", String::as_str);

    let state = std::env::temp_dir().join("hashlark-lan-send.json");
    let secrets: Arc<dyn SecretStore> = Arc::new(FileSecretStore::new(&state));
    let config = LanConfig {
        port: 0,
        discovery: false,
        ..LanConfig::new("lan-send (test)", Platform::Android)
    };
    let lan = LanService::start(config, secrets, Arc::new(Print)).await?;

    let device = lan.add_address(addr).await?;
    println!(
        "found {} ({})",
        device.name,
        if device.paired {
            "paired"
        } else {
            "not paired"
        }
    );
    if pin != "-" {
        lan.pair(&device.id, pin).await?;
        println!("paired");
    }
    lan.send_magnet(&device.id, magnet, title).await?;
    println!("sent");
    lan.shutdown().await;
    Ok(())
}
