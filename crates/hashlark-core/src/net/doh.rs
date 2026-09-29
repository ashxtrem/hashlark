// SPDX-License-Identifier: GPL-3.0-or-later

//! DNS-over-HTTPS (RFC 8484). Name lookups go to the chosen DNS provider
//! over HTTPS instead of the network's resolver, which gets past DNS-based
//! blocking and hides lookups from the network.
//!
//! The well-known providers are contacted by IP address, so no plain DNS
//! lookup ever happens.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use url::Url;

use crate::error::{Error, Result};
use crate::settings::{DohResolver, DohSettings};

const TYPE_A: u16 = 1;
const TYPE_AAAA: u16 = 28;
const CLASS_IN: u16 = 1;
const MIN_TTL: u32 = 30;
const MAX_TTL: u32 = 3600;

/// A DoH endpoint plus fixed addresses for its host.
#[derive(Debug, Clone)]
pub struct Endpoint {
    pub url: Url,
    pub bootstrap: Vec<IpAddr>,
}

impl Endpoint {
    fn known(url: &str, ips: &[IpAddr]) -> Self {
        Self {
            url: url.parse().expect("valid built-in DoH URL"),
            bootstrap: ips.to_vec(),
        }
    }

    pub fn for_settings(settings: &DohSettings) -> Result<Self> {
        let v4 = |a, b, c, d| IpAddr::V4(Ipv4Addr::new(a, b, c, d));
        Ok(match settings.resolver {
            DohResolver::Cloudflare => Self::known(
                "https://cloudflare-dns.com/dns-query",
                &[v4(1, 1, 1, 1), v4(1, 0, 0, 1)],
            ),
            DohResolver::Quad9 => Self::known(
                "https://dns.quad9.net/dns-query",
                &[v4(9, 9, 9, 9), v4(149, 112, 112, 112)],
            ),
            DohResolver::Google => Self::known(
                "https://dns.google/dns-query",
                &[v4(8, 8, 8, 8), v4(8, 8, 4, 4)],
            ),
            DohResolver::Custom => Self {
                url: settings
                    .custom_url
                    .clone()
                    .ok_or_else(|| Error::Config("custom DoH URL missing".into()))?,
                // Resolved once through the system resolver.
                bootstrap: Vec::new(),
            },
        })
    }
}

#[derive(Debug)]
struct CacheEntry {
    expires: Instant,
    ips: Vec<IpAddr>,
}

/// A caching DoH resolver usable by reqwest.
#[derive(Debug)]
pub struct DohResolverClient {
    http: reqwest::Client,
    endpoint: Url,
    fallback_to_system: bool,
    cache: Mutex<HashMap<String, CacheEntry>>,
}

impl DohResolverClient {
    pub fn new(endpoint: Endpoint, fallback_to_system: bool) -> Result<Arc<Self>> {
        let mut builder = reqwest::Client::builder()
            .user_agent(super::USER_AGENT)
            .timeout(Duration::from_secs(5))
            .connect_timeout(Duration::from_secs(4));
        if let Some(host) = endpoint.url.host_str() {
            let port = endpoint.url.port_or_known_default().unwrap_or(443);
            let addrs: Vec<SocketAddr> = endpoint
                .bootstrap
                .iter()
                .map(|ip| SocketAddr::new(*ip, port))
                .collect();
            if !addrs.is_empty() {
                builder = builder.resolve_to_addrs(host, &addrs);
            }
        }
        let http = builder
            .build()
            .map_err(|e| Error::Config(format!("could not build DoH client: {e}")))?;
        Ok(Arc::new(Self {
            http,
            endpoint: endpoint.url,
            fallback_to_system,
            cache: Mutex::default(),
        }))
    }

    async fn query(&self, name: &str, qtype: u16) -> Result<(Vec<IpAddr>, u32), String> {
        let packet = encode_query(name, qtype)?;
        let response = self
            .http
            .post(self.endpoint.clone())
            .header("content-type", "application/dns-message")
            .header("accept", "application/dns-message")
            .body(packet)
            .send()
            .await
            .map_err(|e| format!("DoH request failed: {e}"))?;
        if !response.status().is_success() {
            return Err(format!("DoH server answered HTTP {}", response.status()));
        }
        let body = response
            .bytes()
            .await
            .map_err(|e| format!("DoH response failed: {e}"))?;
        decode_answers(&body, qtype)
    }

    /// Looks up A and AAAA records over DoH, falling back to the system
    /// resolver if allowed.
    pub async fn lookup(&self, name: &str) -> Result<Vec<IpAddr>, String> {
        if let Ok(ip) = name.parse::<IpAddr>() {
            return Ok(vec![ip]);
        }
        let key = name.to_ascii_lowercase();
        if key == "localhost" || key.ends_with(".localhost") {
            return system_lookup(name).await;
        }
        if let Some(entry) = self.cache.lock().expect("lock").get(&key)
            && entry.expires > Instant::now()
        {
            return Ok(entry.ips.clone());
        }

        let (v4, v6) = tokio::join!(self.query(&key, TYPE_A), self.query(&key, TYPE_AAAA));
        let mut ips = Vec::new();
        let mut ttl = MAX_TTL;
        let mut error = None;
        for result in [v4, v6] {
            match result {
                Ok((found, t)) => {
                    if !found.is_empty() {
                        ttl = ttl.min(t);
                    }
                    ips.extend(found);
                }
                Err(e) => error = Some(e),
            }
        }
        if ips.is_empty() {
            let reason = error.unwrap_or_else(|| format!("no address found for {name}"));
            if self.fallback_to_system {
                tracing::debug!(%name, %reason, "DoH failed, using the system resolver");
                return system_lookup(name).await;
            }
            return Err(reason);
        }
        // Prefer IPv4 first: many home networks have broken IPv6.
        ips.sort_by_key(|ip| ip.is_ipv6());
        self.cache.lock().expect("lock").insert(
            key,
            CacheEntry {
                expires: Instant::now() + Duration::from_secs(ttl.clamp(MIN_TTL, MAX_TTL).into()),
                ips: ips.clone(),
            },
        );
        Ok(ips)
    }
}

async fn system_lookup(name: &str) -> Result<Vec<IpAddr>, String> {
    tokio::net::lookup_host((name, 0))
        .await
        .map(|addrs| addrs.map(|a| a.ip()).collect())
        .map_err(|e| format!("DNS lookup failed: {e}"))
}

/// Adapter so reqwest uses the DoH resolver.
#[derive(Debug, Clone)]
pub struct DohResolve(pub Arc<DohResolverClient>);

impl Resolve for DohResolve {
    fn resolve(&self, name: Name) -> Resolving {
        let resolver = Arc::clone(&self.0);
        Box::pin(async move {
            let ips = resolver.lookup(name.as_str()).await.map_err(
                |e| -> Box<dyn std::error::Error + Send + Sync> {
                    format!("dns error: {e}").into()
                },
            )?;
            let addrs: Addrs = Box::new(ips.into_iter().map(|ip| SocketAddr::new(ip, 0)));
            Ok(addrs)
        })
    }
}

/// Encodes a DNS query for `name` (RFC 1035 wire format, id 0 as RFC 8484
/// recommends).
pub fn encode_query(name: &str, qtype: u16) -> Result<Vec<u8>, String> {
    let mut packet = vec![
        0, 0, // id
        0x01, 0x00, // flags: recursion desired
        0, 1, // one question
        0, 0, 0, 0, 0, 0, // no answer/authority/additional records
    ];
    let name = name.trim_end_matches('.');
    if name.is_empty() || name.len() > 253 {
        return Err(format!("invalid host name `{name}`"));
    }
    for label in name.split('.') {
        let bytes = label.as_bytes();
        if bytes.is_empty() || bytes.len() > 63 {
            return Err(format!("invalid host name `{name}`"));
        }
        packet.push(bytes.len() as u8);
        packet.extend_from_slice(bytes);
    }
    packet.push(0);
    packet.extend_from_slice(&qtype.to_be_bytes());
    packet.extend_from_slice(&CLASS_IN.to_be_bytes());
    Ok(packet)
}

fn read_u16(buf: &[u8], at: usize) -> Result<u16, String> {
    buf.get(at..at + 2)
        .map(|b| u16::from_be_bytes([b[0], b[1]]))
        .ok_or_else(|| "truncated DNS response".to_owned())
}

/// Skips an encoded name (labels and/or a compression pointer).
fn skip_name(buf: &[u8], mut at: usize) -> Result<usize, String> {
    loop {
        let len = *buf.get(at).ok_or("truncated DNS name")?;
        match len {
            0 => return Ok(at + 1),
            l if l & 0xC0 == 0xC0 => return Ok(at + 2),
            l => at += 1 + usize::from(l),
        }
    }
}

/// Extracts addresses of type `qtype` and the smallest TTL from a response.
pub fn decode_answers(buf: &[u8], qtype: u16) -> Result<(Vec<IpAddr>, u32), String> {
    if buf.len() < 12 {
        return Err("truncated DNS response".into());
    }
    let rcode = buf[3] & 0x0F;
    // 3 = NXDOMAIN: the name doesn't exist; not an error worth retrying.
    if rcode == 3 {
        return Ok((Vec::new(), MIN_TTL));
    }
    if rcode != 0 {
        return Err(format!("DNS server error (code {rcode})"));
    }
    let questions = read_u16(buf, 4)?;
    let answers = read_u16(buf, 6)?;
    let mut at = 12;
    for _ in 0..questions {
        at = skip_name(buf, at)? + 4;
    }
    let mut ips = Vec::new();
    let mut ttl = MAX_TTL;
    for _ in 0..answers {
        at = skip_name(buf, at)?;
        let rtype = read_u16(buf, at)?;
        let record_ttl = buf
            .get(at + 4..at + 8)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
            .ok_or("truncated DNS record")?;
        let len = usize::from(read_u16(buf, at + 8)?);
        let data = buf
            .get(at + 10..at + 10 + len)
            .ok_or("truncated DNS record")?;
        at += 10 + len;
        // Every record in the chain (CNAMEs too) bounds how long it's valid.
        ttl = ttl.min(record_ttl);
        let ip = match (rtype, len) {
            (TYPE_A, 4) if qtype == TYPE_A => {
                IpAddr::V4(Ipv4Addr::new(data[0], data[1], data[2], data[3]))
            }
            (TYPE_AAAA, 16) if qtype == TYPE_AAAA => {
                let octets: [u8; 16] = data.try_into().expect("length checked");
                IpAddr::V6(Ipv6Addr::from(octets))
            }
            // CNAMEs and others: the resolver already followed them.
            _ => continue,
        };
        ips.push(ip);
    }
    Ok((ips, ttl))
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, Request, ResponseTemplate};

    use super::*;

    /// Builds a response to `query` with the given A/AAAA records and a CNAME
    /// in front, using name compression like real servers do.
    fn response(query: &[u8], ips: &[IpAddr], ttl: u32) -> Vec<u8> {
        let qtype = u16::from_be_bytes([query[query.len() - 4], query[query.len() - 3]]);
        let mut out = query.to_vec();
        out[2] = 0x81;
        out[3] = 0x80;
        let matching: Vec<&IpAddr> = ips
            .iter()
            .filter(|ip| (qtype == TYPE_A) == ip.is_ipv4())
            .collect();
        let count = matching.len() as u16 + 1;
        out[6..8].copy_from_slice(&count.to_be_bytes());
        // CNAME pointing at the question name (pointer 0xC00C).
        out.extend_from_slice(&[0xC0, 0x0C, 0, 5, 0, 1, 0, 0, 0, 60, 0, 2, 0xC0, 0x0C]);
        for ip in matching {
            out.extend_from_slice(&[0xC0, 0x0C]);
            let (rtype, bytes): (u16, Vec<u8>) = match ip {
                IpAddr::V4(v4) => (TYPE_A, v4.octets().to_vec()),
                IpAddr::V6(v6) => (TYPE_AAAA, v6.octets().to_vec()),
            };
            out.extend_from_slice(&rtype.to_be_bytes());
            out.extend_from_slice(&CLASS_IN.to_be_bytes());
            out.extend_from_slice(&ttl.to_be_bytes());
            out.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
            out.extend_from_slice(&bytes);
        }
        out
    }

    #[test]
    fn encodes_queries() {
        let q = encode_query("example.org.", TYPE_A).unwrap();
        assert_eq!(&q[..6], &[0, 0, 1, 0, 0, 1]);
        assert_eq!(&q[12..], b"\x07example\x03org\x00\x00\x01\x00\x01");
        assert!(encode_query("", TYPE_A).is_err());
        assert!(encode_query("a..b", TYPE_A).is_err());
        assert!(encode_query(&"x".repeat(64), TYPE_A).is_err());
    }

    #[test]
    fn decodes_answers_with_cname_and_compression() {
        let ip: IpAddr = "93.184.215.14".parse().unwrap();
        let q = encode_query("example.org", TYPE_A).unwrap();
        let (ips, ttl) = decode_answers(&response(&q, &[ip], 300), TYPE_A).unwrap();
        assert_eq!(ips, vec![ip]);
        assert_eq!(ttl, 60, "smallest TTL, including the CNAME");

        let mut nx = q.clone();
        nx[3] = 0x83;
        assert!(decode_answers(&nx, TYPE_A).unwrap().0.is_empty());
        let mut fail = q.clone();
        fail[3] = 0x82;
        assert!(decode_answers(&fail, TYPE_A).is_err());
        assert!(decode_answers(&[0, 1], TYPE_A).is_err());
    }

    #[tokio::test]
    async fn resolves_over_https_endpoint_and_caches() {
        let server = MockServer::start().await;
        let v4: IpAddr = "10.1.2.3".parse().unwrap();
        let v6: IpAddr = "2001:db8::1".parse().unwrap();
        Mock::given(method("POST"))
            .and(path("/dns-query"))
            .and(header("content-type", "application/dns-message"))
            .respond_with(move |req: &Request| {
                ResponseTemplate::new(200).set_body_bytes(response(&req.body, &[v4, v6], 120))
            })
            .expect(2) // A + AAAA once; the second lookup is cached
            .mount(&server)
            .await;

        let resolver = DohResolverClient::new(
            Endpoint {
                url: format!("{}/dns-query", server.uri()).parse().unwrap(),
                bootstrap: vec![],
            },
            false,
        )
        .unwrap();
        assert_eq!(resolver.lookup("site.example").await.unwrap(), vec![v4, v6]);
        assert_eq!(resolver.lookup("SITE.example").await.unwrap(), vec![v4, v6]);
        assert_eq!(
            resolver.lookup("192.0.2.1").await.unwrap(),
            vec!["192.0.2.1".parse::<IpAddr>().unwrap()]
        );
    }

    #[tokio::test]
    async fn failure_without_fallback_is_an_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
        let resolver = DohResolverClient::new(
            Endpoint {
                url: format!("{}/dns-query", server.uri()).parse().unwrap(),
                bootstrap: vec![],
            },
            false,
        )
        .unwrap();
        assert!(resolver.lookup("site.example").await.is_err());
    }
}
