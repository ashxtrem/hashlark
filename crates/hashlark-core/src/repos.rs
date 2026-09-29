// SPDX-License-Identifier: GPL-3.0-or-later

//! Definition repositories: signed collections of provider definitions that
//! Hashlark keeps up to date (PLAN §7.2).
//!
//! A repository is a folder served over HTTPS:
//!
//! ```text
//! index.json        the list of definitions (see [`RepoIndex`])
//! index.json.sig    Ed25519 signature of index.json's exact bytes, base64
//! <id>.yml ...      the definitions, each checked against its sha256
//! ```
//!
//! The index names its public key. The key seen when a repository is first
//! added is remembered, and later updates signed with any other key are
//! refused.

use data_encoding::BASE64;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::definition::store::sha256_hex;
use crate::error::{Error, Result};

const KEY_PREFIX: &str = "ed25519:";

/// `index.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepoIndex {
    pub schema: u32,
    pub name: String,
    /// `ed25519:<base64 of the 32-byte public key>`.
    pub public_key: String,
    /// Increases with every published change.
    pub version: u64,
    pub definitions: Vec<RepoEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepoEntry {
    pub id: String,
    pub version: u32,
    /// Relative to `index.json`.
    pub url: String,
    pub sha256: String,
}

fn decode_32(text: &str, what: &str) -> Result<[u8; 32]> {
    BASE64
        .decode(text.trim().as_bytes())
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| Error::Invalid(format!("the {what} is not 32 base64-encoded bytes")))
}

/// A new signing key pair as text: `(signing key, public key)`.
pub fn generate_keypair() -> (String, String) {
    let mut seed = [0u8; 32];
    getrandom::fill(&mut seed).expect("OS random number generator");
    let key = SigningKey::from_bytes(&seed);
    (
        BASE64.encode(&seed),
        format!(
            "{KEY_PREFIX}{}",
            BASE64.encode(key.verifying_key().as_bytes())
        ),
    )
}

/// The public key for a signing key.
pub fn public_key_of(signing: &str) -> Result<String> {
    let key = SigningKey::from_bytes(&decode_32(signing, "signing key")?);
    Ok(format!(
        "{KEY_PREFIX}{}",
        BASE64.encode(key.verifying_key().as_bytes())
    ))
}

fn parse_public_key(text: &str) -> Result<VerifyingKey> {
    let b64 = text
        .trim()
        .strip_prefix(KEY_PREFIX)
        .ok_or_else(|| Error::Invalid(format!("public keys start with `{KEY_PREFIX}`")))?;
    VerifyingKey::from_bytes(&decode_32(b64, "public key")?)
        .map_err(|_| Error::Invalid("invalid Ed25519 public key".into()))
}

/// Signs `bytes`, returning the base64 text for `index.json.sig`.
pub fn sign(signing: &str, bytes: &[u8]) -> Result<String> {
    let key = SigningKey::from_bytes(&decode_32(signing, "signing key")?);
    Ok(BASE64.encode(&key.sign(bytes).to_bytes()))
}

/// Checks a base64 signature over `bytes`.
pub fn verify(public_key: &str, bytes: &[u8], signature: &str) -> Result<()> {
    let key = parse_public_key(public_key)?;
    let sig: [u8; 64] = BASE64
        .decode(signature.trim().as_bytes())
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| Error::Invalid("the signature is malformed".into()))?;
    key.verify(bytes, &Signature::from_bytes(&sig))
        .map_err(|_| Error::Invalid("the repository signature does not match".into()))
}

/// Short, human-comparable form of a public key.
pub fn fingerprint(public_key: &str) -> String {
    let hex = sha256_hex(public_key.as_bytes());
    hex.as_bytes()
        .chunks(4)
        .take(4)
        .map(|c| std::str::from_utf8(c).expect("hex is ascii"))
        .collect::<Vec<_>>()
        .join(":")
}

/// Parses a fetched index and checks its signature. `pinned` is the key
/// remembered from the first sync, if any.
pub fn verify_index(bytes: &[u8], signature: &str, pinned: Option<&str>) -> Result<RepoIndex> {
    let index: RepoIndex = serde_json::from_slice(bytes)
        .map_err(|e| Error::Invalid(format!("index.json is invalid: {e}")))?;
    if index.schema != 1 {
        return Err(Error::Invalid(format!(
            "unsupported repository schema {}",
            index.schema
        )));
    }
    if let Some(pinned) = pinned
        && pinned != index.public_key
    {
        return Err(Error::Invalid(format!(
            "the repository's signing key changed (was {}, now {}); remove and re-add it only if you trust the new key",
            fingerprint(pinned),
            fingerprint(&index.public_key)
        )));
    }
    verify(&index.public_key, bytes, signature)?;
    Ok(index)
}

/// Builds `index.json` for a folder of definitions given as
/// `(file name, yaml)`.
pub fn build_index(
    name: &str,
    version: u64,
    public_key: &str,
    files: &[(String, String)],
) -> Result<RepoIndex> {
    let mut definitions = Vec::new();
    for (file, yaml) in files {
        let def = crate::definition::load(yaml)
            .map_err(|e| Error::Invalid(format!("{file}: {e}")))?
            .spec;
        definitions.push(RepoEntry {
            id: def.id,
            version: def.version,
            url: file.clone(),
            sha256: sha256_hex(yaml.as_bytes()),
        });
    }
    definitions.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(RepoIndex {
        schema: 1,
        name: name.to_owned(),
        public_key: public_key.to_owned(),
        version,
        definitions,
    })
}

/// URL of an entry, relative to the index.
pub fn entry_url(index_url: &Url, entry: &RepoEntry) -> Result<Url> {
    let url = index_url
        .join(&entry.url)
        .map_err(|e| Error::Invalid(format!("bad definition URL `{}`: {e}", entry.url)))?;
    if matches!(url.scheme(), "http" | "https") {
        Ok(url)
    } else {
        Err(Error::Invalid(format!(
            "definition URL `{url}` is not http(s)"
        )))
    }
}

/// The index URL for what the user typed: a folder or an `index.json`.
pub fn normalize_repo_url(input: &str) -> Result<Url> {
    let mut url = Url::parse(input.trim())
        .map_err(|_| Error::Invalid("the repository URL is not a valid URL".into()))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(Error::Invalid("the repository URL must be http(s)".into()));
    }
    if !url.path().ends_with(".json") {
        if !url.path().ends_with('/') {
            url.set_path(&format!("{}/", url.path()));
        }
        url = url.join("index.json").expect("relative join");
    }
    Ok(url)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const DEF: &str = "schema: 1\nid: repo-site\nname: Repo Site\nversion: 3\nlinks: [https://x.example/]\nsearch:\n  path: /s\n  response: html\n  rows: tr\n  fields:\n    title: { selector: a }\n    magnet: { selector: a, attr: href }\n";

    #[test]
    fn signing_and_pinning() {
        let (signing, public) = generate_keypair();
        assert_eq!(public_key_of(&signing).unwrap(), public);
        let index =
            build_index("Test", 7, &public, &[("repo-site.yml".into(), DEF.into())]).unwrap();
        assert_eq!(index.definitions[0].version, 3);
        let bytes = serde_json::to_vec_pretty(&index).unwrap();
        let sig = sign(&signing, &bytes).unwrap();

        assert_eq!(verify_index(&bytes, &sig, None).unwrap(), index);
        assert_eq!(
            verify_index(&bytes, &sig, Some(&public)).unwrap().version,
            7
        );

        let (_, other) = generate_keypair();
        let err = verify_index(&bytes, &sig, Some(&other))
            .unwrap_err()
            .to_string();
        assert!(err.contains("signing key changed"), "{err}");

        // Change only the version number (the key is random and may contain a 7).
        let tampered = String::from_utf8(bytes.clone())
            .unwrap()
            .replace("\"version\": 7", "\"version\": 8")
            .into_bytes();
        assert_ne!(tampered, bytes);
        let err = verify_index(&tampered, &sig, None).unwrap_err().to_string();
        assert!(err.contains("signature"), "{err}");
    }

    #[test]
    fn urls_and_fingerprints() {
        assert_eq!(
            normalize_repo_url("https://r.example/defs")
                .unwrap()
                .as_str(),
            "https://r.example/defs/index.json"
        );
        assert_eq!(
            normalize_repo_url("https://r.example/x/index.json")
                .unwrap()
                .as_str(),
            "https://r.example/x/index.json"
        );
        assert!(normalize_repo_url("file:///etc").is_err());
        let base: Url = "https://r.example/defs/index.json".parse().unwrap();
        let entry = RepoEntry {
            id: "a".into(),
            version: 1,
            url: "a.yml".into(),
            sha256: String::new(),
        };
        assert_eq!(
            entry_url(&base, &entry).unwrap().as_str(),
            "https://r.example/defs/a.yml"
        );
        assert_eq!(fingerprint("ed25519:abc").len(), 19);
    }

    #[test]
    fn build_rejects_invalid_definitions() {
        let (_, public) = generate_keypair();
        assert!(build_index("T", 1, &public, &[("bad.yml".into(), "schema: 1".into())]).is_err());
    }
}
