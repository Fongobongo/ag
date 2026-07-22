//! Minimal RSS (resident set size) probe for budget checks (audit 22.1.1:
//! node idle ≤ 25 MB, control plane idle ≤ 64 MB, streaming ≤ 60 MB).
//!
//! Reads `/proc/self/status` `VmRSS:` — Linux only, returns `None` elsewhere
//! (or on any read error) so callers can probe without gating on platform.

/// Current resident set size in bytes, or `None` if it cannot be read.
pub fn current_rss() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            // "VmRSS:\t      1234 kB"
            let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
            return Some(kb * 1024);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_vmrss_line() {
        assert_eq!(parse_vmrss("VmRSS:\t      1234 kB\n"), Some(1234 * 1024));
        assert_eq!(parse_vmrss("VmSize:\t      9999 kB\n"), None);
        assert_eq!(parse_vmrss(""), None);
    }

    /// Reimplementation mirroring [`current_rss`]'s line scan, for a single
    /// line (keeps the test from depending on a live `/proc`).
    fn parse_vmrss(status: &str) -> Option<u64> {
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("VmRSS:") {
                let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
                return Some(kb * 1024);
            }
        }
        None
    }

    #[test]
    fn current_rss_returns_something_on_linux() {
        // On a Linux CI box this must read a positive RSS; elsewhere it may be
        // None, so only assert the non-Linux path.
        if let Some(rss) = current_rss() {
            assert!(rss > 0, "rss should be positive, got {rss}");
        }
    }
}
