pub struct BuildInfo {
    pub version: &'static str,
    pub channel: &'static str,
    pub git_sha: &'static str,
    pub git_date: &'static str,
    pub dirty: &'static str,
    pub target: &'static str,
}

impl BuildInfo {
    /// Nightly has no meaningful semver (`Cargo.toml` never bumps between
    /// nightlies), so callers show date+commit instead.
    pub fn is_nightly(&self) -> bool {
        self.channel == "nightly"
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty == "dirty"
    }
}
