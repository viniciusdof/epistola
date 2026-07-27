include!("../../../build-support/types.rs");

pub const BUILD_INFO: BuildInfo = BuildInfo {
    version: env!("CARGO_PKG_VERSION"),
    channel: env!("EPISTOLA_BUILD_CHANNEL"),
    git_sha: env!("EPISTOLA_BUILD_GIT_SHA"),
    git_date: env!("EPISTOLA_BUILD_GIT_DATE"),
    dirty: env!("EPISTOLA_BUILD_GIT_DIRTY"),
    target: env!("EPISTOLA_BUILD_TARGET"),
};

fn sha_with_dirty_suffix(info: &BuildInfo) -> String {
    if info.is_dirty() {
        format!("{}-dirty", info.git_sha)
    } else {
        info.git_sha.to_string()
    }
}

pub fn short_version(info: &BuildInfo) -> String {
    if info.is_nightly() {
        format!(
            "nightly {} ({})",
            info.git_date,
            sha_with_dirty_suffix(info)
        )
    } else {
        info.version.to_string()
    }
}

pub fn long_version(info: &BuildInfo) -> String {
    if info.is_nightly() {
        format!(
            "nightly {} ({})\ntarget: {}",
            info.git_date,
            sha_with_dirty_suffix(info),
            info.target
        )
    } else {
        format!(
            "{}\ncommit: {} ({})\ntarget: {}",
            info.version,
            sha_with_dirty_suffix(info),
            info.git_date,
            info.target
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NIGHTLY: BuildInfo = BuildInfo {
        version: "0.1.0",
        channel: "nightly",
        git_sha: "abc1234",
        git_date: "2026-07-27",
        dirty: "clean",
        target: "x86_64-unknown-linux-gnu",
    };

    const DEV: BuildInfo = BuildInfo {
        version: "0.1.0",
        channel: "dev",
        git_sha: "abc1234",
        git_date: "2026-07-27",
        dirty: "dirty",
        target: "x86_64-unknown-linux-gnu",
    };

    #[test]
    fn nightly_short_version_omits_semver() {
        assert_eq!(short_version(&NIGHTLY), "nightly 2026-07-27 (abc1234)");
    }

    #[test]
    fn dev_short_version_is_plain_semver() {
        assert_eq!(short_version(&DEV), "0.1.0");
    }

    #[test]
    fn dev_long_version_includes_dirty_suffix_and_target() {
        let out = long_version(&DEV);
        assert!(out.contains("abc1234-dirty"));
        assert!(out.contains("x86_64-unknown-linux-gnu"));
    }

    #[test]
    fn nightly_long_version_has_no_semver_line() {
        let out = long_version(&NIGHTLY);
        assert!(!out.contains("0.1.0"));
        assert!(out.starts_with("nightly 2026-07-27 (abc1234)"));
    }
}
