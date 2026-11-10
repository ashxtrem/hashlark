// SPDX-License-Identifier: GPL-3.0-or-later

//! Finding other Hashlark devices with mDNS (`_hashlark._tcp.local.`).
//!
//! On Android the app must hold a `WifiManager.MulticastLock` while this
//! runs, or the Wi-Fi driver drops the multicast replies.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use mdns_sd::{ResolvedService, ServiceDaemon, ServiceEvent, ServiceInfo};

use crate::protocol::PROTOCOL_VERSION;
use crate::service::{Seen, Shared};
use crate::{Platform, clean_name};

/// The mDNS service type Hashlark advertises.
pub const SERVICE_TYPE: &str = "_hashlark._tcp.local.";

pub(crate) struct Discovery {
    daemon: ServiceDaemon,
    fullname: String,
    task: tokio::task::JoinHandle<()>,
}

impl std::fmt::Debug for Discovery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Discovery")
            .field("fullname", &self.fullname)
            .finish_non_exhaustive()
    }
}

impl Discovery {
    /// Advertises this device and starts browsing for others.
    pub fn start(shared: Arc<Shared>) -> Result<Self, mdns_sd::Error> {
        let hello = &shared.hello;
        let daemon = ServiceDaemon::new()?;
        let short = &hello.id[..6];
        let instance = instance_name(&hello.name, short);
        let host = format!("hashlark-{}.local.", &hello.id[..12]);
        let properties: HashMap<String, String> = [
            ("id", hello.id.clone()),
            ("v", PROTOCOL_VERSION.to_string()),
            ("platform", platform_str(hello.platform).to_owned()),
            ("name", hello.name.clone()),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect();
        let info = ServiceInfo::new(SERVICE_TYPE, &instance, &host, (), hello.port, properties)?
            .enable_addr_auto();
        let fullname = info.get_fullname().to_owned();
        daemon.register(info)?;
        let events = daemon.browse(SERVICE_TYPE)?;
        let task = tokio::spawn(async move {
            while let Ok(event) = events.recv_async().await {
                match event {
                    ServiceEvent::ServiceResolved(service) => found(&shared, &service),
                    ServiceEvent::ServiceRemoved(_, fullname) => lost(&shared, &fullname),
                    _ => {}
                }
            }
        });
        Ok(Self {
            daemon,
            fullname,
            task,
        })
    }

    pub fn stop(self) {
        let _ = self.daemon.unregister(&self.fullname);
        let _ = self.daemon.shutdown();
        self.task.abort();
    }
}

/// mDNS instance names are limited to 63 bytes.
fn instance_name(name: &str, short_id: &str) -> String {
    let suffix = format!(" ({short_id})");
    let mut base = String::new();
    for c in name.chars() {
        if base.len() + c.len_utf8() + suffix.len() > 63 {
            break;
        }
        base.push(c);
    }
    format!("{base}{suffix}")
}

fn platform_str(platform: Platform) -> &'static str {
    match platform {
        Platform::Desktop => "desktop",
        Platform::Android => "android",
    }
}

fn parse_platform(text: Option<&str>) -> Platform {
    match text {
        Some("android") => Platform::Android,
        _ => Platform::Desktop,
    }
}

/// The address to reach a service at: a private IPv4 address when there is
/// one (link-local IPv6 needs a scope id that URLs can't carry).
fn best_addr(service: &ResolvedService) -> Option<IpAddr> {
    let v4: Vec<_> = service
        .get_addresses_v4()
        .into_iter()
        .filter(|ip| !ip.is_loopback() && !ip.is_unspecified())
        .collect();
    v4.iter()
        .find(|ip| ip.is_private())
        .or_else(|| v4.first())
        .copied()
        .map(IpAddr::V4)
}

fn found(shared: &Shared, service: &ResolvedService) {
    let Some(id) = service.get_property_val_str("id") else {
        return;
    };
    if id == shared.hello.id {
        return;
    }
    let version = service
        .get_property_val_str("v")
        .and_then(|v| v.parse::<u32>().ok());
    if version != Some(PROTOCOL_VERSION) {
        tracing::debug!(%id, ?version, "ignoring a device with another protocol version");
        return;
    }
    let Some(ip) = best_addr(service) else {
        return;
    };
    let seen = Seen {
        name: clean_name(service.get_property_val_str("name").unwrap_or_default()),
        platform: parse_platform(service.get_property_val_str("platform")),
        addr: SocketAddr::new(ip, service.get_port()),
        fullname: Some(service.get_fullname().to_owned()),
    };
    if shared.see(id.to_owned(), seen) {
        shared.emit(|events| events.devices_changed());
    }
}

fn lost(shared: &Shared, fullname: &str) {
    if shared.unsee(fullname) {
        shared.emit(|events| events.devices_changed());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_names_fit_in_63_bytes() {
        let long = "é".repeat(80);
        let name = instance_name(&long, "abcdef");
        assert!(name.len() <= 63, "{} bytes", name.len());
        assert!(name.ends_with(" (abcdef)"));
        assert_eq!(instance_name("Desk", "abcdef"), "Desk (abcdef)");
    }
}
