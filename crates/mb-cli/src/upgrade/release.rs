//! Is there a newer `mb`? Releases are the kit's GitHub releases (the kit
//! release pushes a `vX.Y.Z` tag, its release.yml attaches the binaries, and
//! maimbrain.com/install.sh downloads `releases/latest`), so the latest is
//! `GET /repos/terracatta/maimbrain-kit/releases/latest`'s `tag_name`.
//! `mb doctor` asks at most once a day: the answer is kept in mb's config
//! directory (`release-check.json`), offline it says nothing, and
//! `MB_NO_UPDATE_CHECK=1` turns it off. `MB_RELEASES_URL` points it at
//! another server (tests).

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::notes::Version;

const RELEASES_URL: &str = "https://api.github.com/repos/terracatta/maimbrain-kit/releases/latest";
const DAY: u64 = 24 * 60 * 60;

/// The latest release, if known: from the cache when it was asked for in the
/// last day, else from the releases endpoint (None offline or turned off).
pub fn latest() -> Option<Version> {
    if std::env::var("MB_NO_UPDATE_CHECK").is_ok_and(|v| !v.is_empty() && v != "0") {
        return None;
    }
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    let path = cache_path();
    let cached: Option<serde_json::Value> = path.as_ref().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str(&t).ok());
    let cached_latest = cached.as_ref().and_then(|c| c["latest"].as_str()).and_then(Version::parse);
    if let Some(checked) = cached.as_ref().and_then(|c| c["checked"].as_u64())
        && checked <= now
        && now - checked < DAY
    {
        return cached_latest;
    }
    let url = std::env::var("MB_RELEASES_URL").unwrap_or_else(|_| RELEASES_URL.into());
    let fetched = fetch(&url);
    // An attempt counts even when it fails, so an offline doctor doesn't wait on it every time.
    let latest = fetched.or(cached_latest);
    if let Some(p) = path {
        let _ = std::fs::create_dir_all(p.parent().unwrap_or(&p));
        let _ = std::fs::write(&p, serde_json::json!({ "checked": now, "latest": latest.as_ref().map(|v| v.to_string()) }).to_string());
    }
    latest
}

fn cache_path() -> Option<PathBuf> {
    crate::remote::config_dir().ok().map(|d| d.join("release-check.json"))
}

fn fetch(url: &str) -> Option<Version> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(4)))
        .user_agent(concat!("mb/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();
    let mut resp = agent.get(url).header("Accept", "application/vnd.github+json").call().ok()?;
    if resp.status().as_u16() != 200 {
        return None;
    }
    let body: serde_json::Value = serde_json::from_str(&resp.body_mut().read_to_string().ok()?).ok()?;
    body["tag_name"].as_str().and_then(Version::parse)
}

/// `mb doctor`'s line about it: (all's well, the line, how to update).
pub fn doctor_line() -> Option<(bool, String, String)> {
    let latest = latest()?;
    let current = Version::current();
    if latest <= current {
        return Some((true, format!("mb {current} is the latest release"), String::new()));
    }
    let install = if cfg!(windows) { "irm https://maimbrain.com/install.ps1 | iex" } else { "curl -fsSL https://maimbrain.com/install.sh | sh" };
    Some((
        false,
        format!("mb {current}: {latest} is out"),
        format!("{install}; in Claude Code, /plugin update maimbrain@maimbrain-kit for the skill; then mb upgrade <game dir> for each game"),
    ))
}
