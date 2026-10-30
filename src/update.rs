//! Checking for a newer drift: the latest release on GitHub, against this build's
//! version.

/// Where drift's releases are published.
const RELEASES: &str = "https://api.github.com/repos/domknez/drift/releases/latest";

/// What a check found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Found {
    /// This is the latest.
    Current,
    /// A newer version: its number and the page to get it from.
    Newer { version: String, page: String },
    /// Nothing has been released yet.
    NoReleases,
}

/// This build's version.
pub fn current() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Ask GitHub for the latest release. Blocks; run it off the UI thread.
pub fn check() -> Result<Found, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(15)))
        .http_status_as_error(false)
        .user_agent(concat!("drift/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();
    let response = agent.get(RELEASES).header("Accept", "application/vnd.github+json").call().map_err(|e| e.to_string())?;
    match response.status().as_u16() {
        404 => return Ok(Found::NoReleases),
        200..=299 => {}
        status => return Err(format!("GitHub answered {status}")),
    }
    let release: serde_json::Value = response.into_body().read_json().map_err(|e| e.to_string())?;
    let tag = release["tag_name"].as_str().ok_or("no version in the answer")?;
    let page = release["html_url"].as_str().unwrap_or("https://github.com/domknez/drift/releases").to_string();
    Ok(if newer(tag, current()) { Found::Newer { version: tag.trim_start_matches('v').to_string(), page } } else { Found::Current })
}

/// Whether version `a` (`v1.2.3` or `1.2.3`) is newer than `b`.
fn newer(a: &str, b: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> {
        v.trim_start_matches('v').split(['.', '-', '+']).take(3).map(|p| p.parse().unwrap_or(0)).collect()
    };
    parts(a) > parts(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(newer("v0.2.0", "0.1.0"));
        assert!(newer("0.10.0", "0.9.9"));
        assert!(!newer("v0.1.0", "0.1.0"));
        assert!(!newer("0.0.9", "0.1.0"));
    }
}
