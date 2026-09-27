//! `ta upgrade`: replace this binary with the newest release, fetched with
//! the player's own `gh` login (the repository's releases are not public).

use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context};
use sha2::{Digest, Sha256};

pub const DEFAULT_REPO: &str = "renderedstep/terminal";
pub const ENV_REPO: &str = "TA_RELEASE_REPO";
pub const TARGET: &str = env!("TA_TARGET");
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

pub fn repo() -> String {
    std::env::var(ENV_REPO)
        .ok()
        .filter(|r| !r.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_REPO.to_string())
}

/// The release asset holding the binary for `target`.
pub fn asset_name(target: &str) -> String {
    if target.contains("windows") {
        format!("ta-{target}.zip")
    } else {
        format!("ta-{target}.tar.gz")
    }
}

/// `1.2.3` from `v1.2.3`, as numbers for comparison.
pub fn parse_version(v: &str) -> Option<Vec<u64>> {
    let v = v.trim().trim_start_matches('v');
    let core = v.split(['-', '+']).next()?;
    core.split('.').map(|p| p.parse().ok()).collect()
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

/// Find the expected digest for `asset` in a `SHA256SUMS` file.
pub fn expected_digest(sums: &str, asset: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let digest = parts.next()?;
        let name = parts.next()?.trim_start_matches('*');
        (name == asset).then(|| digest.to_lowercase())
    })
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn gh(args: &[&str]) -> anyhow::Result<String> {
    let out = Command::new("gh")
        .args(args)
        .output()
        .context("could not run `gh`; install the GitHub CLI and run `gh auth login`")?;
    if !out.status.success() {
        bail!(
            "`gh {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn latest_tag(repo: &str) -> anyhow::Result<String> {
    gh(&[
        "release", "view", "--repo", repo, "--json", "tagName", "--jq", ".tagName",
    ])
}

pub fn run(check_only: bool) -> anyhow::Result<()> {
    let repo = repo();
    let latest = latest_tag(&repo)?;
    if !is_newer(&latest, CURRENT) {
        println!("ta {CURRENT} is the latest release.");
        return Ok(());
    }
    if check_only {
        println!("ta {latest} is available (this is {CURRENT}). Run `ta upgrade` to install it.");
        return Ok(());
    }
    let asset = asset_name(TARGET);
    let dir = tempfile::tempdir()?;
    let dir_str = dir.path().to_string_lossy().to_string();
    gh(&[
        "release",
        "download",
        &latest,
        "--repo",
        &repo,
        "--pattern",
        &asset,
        "--pattern",
        "SHA256SUMS",
        "--dir",
        &dir_str,
    ])?;
    let archive = dir.path().join(&asset);
    let bytes = std::fs::read(&archive).with_context(|| format!("{asset} was not downloaded"))?;
    let sums = std::fs::read_to_string(dir.path().join("SHA256SUMS"))?;
    let expected = expected_digest(&sums, &asset)
        .with_context(|| format!("SHA256SUMS does not list {asset}"))?;
    if sha256_hex(&bytes) != expected {
        bail!("the checksum of {asset} does not match SHA256SUMS; nothing was changed");
    }
    extract(&archive, dir.path())?;
    let exe = if TARGET.contains("windows") {
        "ta.exe"
    } else {
        "ta"
    };
    self_replace::self_replace(dir.path().join(exe))?;
    println!("Upgraded ta {CURRENT} to {latest}.");
    Ok(())
}

fn extract(archive: &Path, into: &Path) -> anyhow::Result<()> {
    // `tar` reads both .tar.gz and .zip on macOS, Linux and Windows 10+.
    let status = Command::new("tar")
        .arg("-xf")
        .arg(archive)
        .arg("-C")
        .arg(into)
        .status()
        .context("could not run `tar` to unpack the release")?;
    if !status.success() {
        bail!("could not unpack {}", archive.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(is_newer("v0.2.0", "0.1.9"));
        assert!(is_newer("v0.10.0", "0.9.0"));
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(!is_newer("garbage", "0.1.0"));
    }

    #[test]
    fn finds_digests() {
        let sums =
            "abc123  ta-x86_64-unknown-linux-gnu.tar.gz\nDEF456 *ta-x86_64-pc-windows-msvc.zip\n";
        assert_eq!(
            expected_digest(sums, "ta-x86_64-pc-windows-msvc.zip").as_deref(),
            Some("def456")
        );
        assert_eq!(expected_digest(sums, "ta-other.tar.gz"), None);
        assert_eq!(
            asset_name("aarch64-apple-darwin"),
            "ta-aarch64-apple-darwin.tar.gz"
        );
    }
}
