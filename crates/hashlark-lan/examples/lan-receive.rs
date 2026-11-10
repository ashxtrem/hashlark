// SPDX-License-Identifier: GPL-3.0-or-later

//! Plays the computer's part from a terminal, for trying the phone's
//! "Send to computer": it shows a PIN, waits for one link and prints it
//! instead of opening a torrent client.
//!
//! ```text
//! cargo run -p hashlark-lan --example lan-receive -- [bind address] [port]
//! ```
//!
//! The defaults (`127.0.0.1`, 47821) suit an Android emulator, which reaches
//! the host's loopback as `10.0.2.2`; for a real phone pass `0.0.0.0`.

use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use hashlark_core::secrets::{FileSecretStore, SecretStore};
use hashlark_lan::{Device, LanConfig, LanEvents, LanService, Platform, Received};

struct Events(Mutex<mpsc::Sender<Received>>);

impl LanEvents for Events {
    fn received(&self, item: Received) {
        let _ = self.0.lock().expect("lock").send(item);
    }

    fn paired(&self, device: Device) {
        println!("paired with {} ({:?})", device.name, device.platform);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let bind = args.first().map_or("127.0.0.1", String::as_str).parse()?;
    let port = args
        .get(1)
        .map_or(Ok(hashlark_lan::DEFAULT_PORT), |p| p.parse())?;

    let state = std::env::temp_dir().join("hashlark-lan-receive.json");
    let secrets: Arc<dyn SecretStore> = Arc::new(FileSecretStore::new(&state));
    let (tx, rx) = mpsc::channel();
    let config = LanConfig {
        bind,
        port,
        discovery: false,
        ..LanConfig::new("lan-receive (test)", Platform::Desktop)
    };
    let lan = LanService::start(config, secrets, Arc::new(Events(Mutex::new(tx)))).await?;
    let code = lan.open_pairing();
    println!("listening on {bind}:{}", lan.port());
    println!("PIN {}", code.pin);

    let received =
        tokio::task::spawn_blocking(move || rx.recv_timeout(Duration::from_secs(600))).await?;
    match received {
        Ok(item) => println!(
            "received “{}” from {}: {}",
            item.title, item.from.name, item.url
        ),
        Err(_) => println!("nothing received"),
    }
    lan.shutdown().await;
    Ok(())
}
