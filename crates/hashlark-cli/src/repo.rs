// SPDX-License-Identifier: GPL-3.0-or-later

//! `hashlark-cli repo …`: publish and use definition repositories.

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use clap::Subcommand;
use hashlark_core::Engine;
use hashlark_core::engine::SyncReport;
use hashlark_core::repos;

#[derive(Debug, Subcommand)]
pub enum RepoCommand {
    /// Create a signing key for publishing a repository.
    Keygen {
        /// Where to save the signing key. Keep it private.
        #[arg(short, long, default_value = "repo-signing.key")]
        output: PathBuf,
    },
    /// Write a signed index.json (and index.json.sig) for a folder of definitions.
    Build {
        /// Folder containing the .yml definitions.
        dir: PathBuf,
        /// Signing key file from `repo keygen`.
        #[arg(long)]
        key: PathBuf,
        /// Repository name shown to users.
        #[arg(long)]
        name: String,
        /// Repository version; must increase with every release.
        #[arg(long)]
        version: u64,
    },
    /// Add a repository to your Hashlark.
    Add { url: String },
    /// List added repositories.
    List,
    /// Fetch a repository's latest definitions.
    Sync { id: String },
    /// Remove a repository and its providers.
    Remove { id: String },
}

pub async fn run(
    command: RepoCommand,
    open: impl AsyncFnOnce() -> anyhow::Result<Engine>,
) -> anyhow::Result<()> {
    match command {
        RepoCommand::Keygen { output } => keygen(&output),
        RepoCommand::Build {
            dir,
            key,
            name,
            version,
        } => build(&dir, &key, &name, version),
        RepoCommand::Add { url } => {
            let engine = open().await?;
            let (repo, report) = engine.add_repo(&url).await?;
            println!(
                "Added {} ({})",
                repo.name.as_deref().unwrap_or("repository"),
                repo.id
            );
            println!(
                "Signing key fingerprint: {}",
                repo.fingerprint.unwrap_or_default()
            );
            print_report(&report);
            if !report.added.is_empty() {
                println!("New providers start disabled; enable them in the app.");
            }
            Ok(())
        }
        RepoCommand::List => {
            let engine = open().await?;
            println!(
                "{:<16}  {:<28}  {:>5}  {:<19}  URL",
                "ID", "NAME", "DEFS", "KEY"
            );
            for r in engine.repos().await? {
                println!(
                    "{:<16}  {:<28}  {:>5}  {:<19}  {}",
                    r.id,
                    r.name.unwrap_or_default(),
                    r.definitions,
                    r.fingerprint.unwrap_or_else(|| "-".into()),
                    r.url
                );
            }
            Ok(())
        }
        RepoCommand::Sync { id } => {
            let engine = open().await?;
            print_report(&engine.sync_repo(&id).await?);
            Ok(())
        }
        RepoCommand::Remove { id } => {
            let engine = open().await?;
            engine.remove_repo(&id).await?;
            println!("Removed {id}");
            Ok(())
        }
    }
}

fn print_report(report: &SyncReport) {
    println!(
        "{} added, {} updated, {} removed, {} unchanged",
        report.added.len(),
        report.updated.len(),
        report.removed.len(),
        report.unchanged
    );
    for error in &report.errors {
        println!("  skipped: {error}");
    }
}

fn keygen(output: &Path) -> anyhow::Result<()> {
    if output.exists() {
        bail!("{} already exists", output.display());
    }
    let (signing, public) = repos::generate_keypair();
    std::fs::write(output, format!("{signing}\n"))
        .with_context(|| format!("writing {}", output.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(output, std::fs::Permissions::from_mode(0o600))?;
    }
    println!(
        "Signing key saved to {} — keep it private and backed up.",
        output.display()
    );
    println!("Public key: {public}");
    println!("Fingerprint: {}", repos::fingerprint(&public));
    Ok(())
}

fn build(dir: &Path, key_file: &Path, name: &str, version: u64) -> anyhow::Result<()> {
    let signing = std::fs::read_to_string(key_file)
        .with_context(|| format!("reading {}", key_file.display()))?;
    let public = repos::public_key_of(&signing)?;
    let mut files = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| matches!(p.extension().and_then(|e| e.to_str()), Some("yml" | "yaml")))
        .collect();
    entries.sort();
    for path in entries {
        let file = path
            .file_name()
            .and_then(|f| f.to_str())
            .context("non-UTF-8 file name")?
            .to_owned();
        files.push((file, std::fs::read_to_string(&path)?));
    }
    if files.is_empty() {
        bail!("no .yml definitions in {}", dir.display());
    }
    let index = repos::build_index(name, version, &public, &files)?;
    let bytes = serde_json::to_vec_pretty(&index)?;
    std::fs::write(dir.join("index.json"), &bytes)?;
    std::fs::write(dir.join("index.json.sig"), repos::sign(&signing, &bytes)?)?;
    println!(
        "Wrote index.json and index.json.sig for {} definition(s), version {version}.",
        index.definitions.len()
    );
    println!("Publish the folder over HTTPS; users add its URL in Hashlark.");
    Ok(())
}
