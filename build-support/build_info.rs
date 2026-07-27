/// Read by `embed_icon`, which only exists on Windows — other platforms see
/// these fields as dead code.
#[cfg_attr(not(windows), allow(dead_code))]
struct BuildEnv {
    channel: String,
    git_sha: String,
    git_date: String,
    dirty: bool,
}

fn emit_build_info_env() -> BuildEnv {
    let channel = std::env::var("EPISTOLA_CHANNEL").unwrap_or_else(|_| "dev".to_string());
    let (git_sha, git_date) = git_sha_and_date();
    let dirty = is_dirty();
    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());

    println!("cargo:rerun-if-env-changed=EPISTOLA_CHANNEL");
    println!("cargo:rerun-if-env-changed=EPISTOLA_GIT_SHA");
    println!("cargo:rerun-if-env-changed=EPISTOLA_GIT_DATE");
    println!("cargo:rustc-env=EPISTOLA_BUILD_CHANNEL={channel}");
    println!("cargo:rustc-env=EPISTOLA_BUILD_GIT_SHA={git_sha}");
    println!("cargo:rustc-env=EPISTOLA_BUILD_GIT_DATE={git_date}");
    println!(
        "cargo:rustc-env=EPISTOLA_BUILD_GIT_DIRTY={}",
        if dirty { "dirty" } else { "clean" }
    );
    println!("cargo:rustc-env=EPISTOLA_BUILD_TARGET={target}");

    BuildEnv {
        channel,
        git_sha,
        git_date,
        dirty,
    }
}

/// CI passes `EPISTOLA_GIT_SHA`/`EPISTOLA_GIT_DATE` explicitly so a
/// `rust-cache` hit still forces a rebuild when the commit changes. Falls
/// back to `git`, then to "unknown" (e.g. a source tarball with no `.git`).
fn git_sha_and_date() -> (String, String) {
    let sha = std::env::var("EPISTOLA_GIT_SHA")
        .ok()
        .or_else(|| run_git(&["rev-parse", "HEAD"]))
        .map(|full| full.chars().take(7).collect::<String>())
        .unwrap_or_else(|| "unknown".to_string());
    let date = std::env::var("EPISTOLA_GIT_DATE")
        .ok()
        .or_else(|| run_git(&["log", "-1", "--date=format:%Y-%m-%d", "--format=%cd"]))
        .unwrap_or_else(|| "unknown".to_string());
    (sha, date)
}

fn is_dirty() -> bool {
    run_git(&["status", "--porcelain"])
        .map(|out| !out.trim().is_empty())
        .unwrap_or(false)
}

fn run_git(args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!text.is_empty()).then_some(text)
}
