//! `ta upgrade`: replace this binary with the newest release, fetched with
//! the player's `gh` login when there is one and over plain HTTPS otherwise
//! (the repository's releases are public).

use std::path::Path;
use std::process::Command;
use std::time::Duration;

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

/// Whether `gh` is installed and signed in.
fn gh_ready() -> bool {
    Command::new("gh")
        .args(["auth", "status"])
        .output()
        .is_ok_and(|out| out.status.success())
}

fn http() -> anyhow::Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(300))
        .user_agent(concat!(
            "text-adventure-terminal/",
            env!("CARGO_PKG_VERSION")
        ))
        .build()?)
}

fn http_get(client: &reqwest::blocking::Client, url: &str) -> anyhow::Result<Vec<u8>> {
    let resp = client
        .get(url)
        .send()
        .with_context(|| format!("could not fetch {url}"))?;
    if !resp.status().is_success() {
        bail!("{url} answered {}", resp.status());
    }
    Ok(resp.bytes()?.to_vec())
}

pub fn latest_tag(repo: &str, use_gh: bool) -> anyhow::Result<String> {
    if use_gh {
        return gh(&[
            "release", "view", "--repo", repo, "--json", "tagName", "--jq", ".tagName",
        ]);
    }
    let body = http_get(
        &http()?,
        &format!("https://api.github.com/repos/{repo}/releases/latest"),
    )?;
    let release: serde_json::Value = serde_json::from_slice(&body)?;
    release["tag_name"]
        .as_str()
        .map(str::to_string)
        .with_context(|| format!("no latest release found in {repo}"))
}

/// Put `asset` and `SHA256SUMS` from release `tag` into `dir`.
fn download(repo: &str, tag: &str, asset: &str, dir: &Path, use_gh: bool) -> anyhow::Result<()> {
    if use_gh {
        let dir_str = dir.to_string_lossy();
        gh(&[
            "release",
            "download",
            tag,
            "--repo",
            repo,
            "--pattern",
            asset,
            "--pattern",
            "SHA256SUMS",
            "--dir",
            &dir_str,
        ])?;
        return Ok(());
    }
    let client = http()?;
    for name in [asset, "SHA256SUMS"] {
        let url = format!("https://github.com/{repo}/releases/download/{tag}/{name}");
        std::fs::write(dir.join(name), http_get(&client, &url)?)?;
    }
    Ok(())
}

pub fn run(check_only: bool) -> anyhow::Result<()> {
    let repo = repo();
    let use_gh = gh_ready();
    let latest = latest_tag(&repo, use_gh)?;
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
    download(&repo, &latest, &asset, dir.path(), use_gh)?;
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
