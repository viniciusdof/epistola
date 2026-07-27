include!("../../../build-support/types.rs");

pub const BUILD_INFO: BuildInfo = BuildInfo {
    version: env!("CARGO_PKG_VERSION"),
    channel: env!("EPISTOLA_BUILD_CHANNEL"),
    git_sha: env!("EPISTOLA_BUILD_GIT_SHA"),
    git_date: env!("EPISTOLA_BUILD_GIT_DATE"),
    dirty: env!("EPISTOLA_BUILD_GIT_DIRTY"),
    target: env!("EPISTOLA_BUILD_TARGET"),
};
