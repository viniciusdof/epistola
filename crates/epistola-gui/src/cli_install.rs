//! Installs the bundled `epistola` CLI binary onto the user's PATH.

use std::path::{Path, PathBuf};

use gpui::{Context, PromptLevel, Window};

use crate::root::EpistolaGui;

impl EpistolaGui {
    pub(crate) fn install_cli(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |weak, cx| {
            let result = cx.background_executor().spawn(async { install() }).await;
            let prompt = weak.update_in(cx, |_this, window, cx| match result {
                Ok(path) => window.prompt(
                    PromptLevel::Info,
                    "epistola CLI installed",
                    Some(&format!(
                        "Installed at {}. You can now run `epistola` from your terminal.",
                        path.display()
                    )),
                    &["OK"],
                    cx,
                ),
                Err(message) => window.prompt(
                    PromptLevel::Critical,
                    "Couldn't install the CLI",
                    Some(&message),
                    &["OK"],
                    cx,
                ),
            });

            if let Ok(task) = prompt {
                let _ = task.await;
            }
        })
        .detach();
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn install() -> Result<PathBuf, String> {
    let binary = locate_cli_binary()?;
    install_symlink(&binary)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn install() -> Result<PathBuf, String> {
    Err(
        "Installing the CLI from the app isn't supported on this platform yet — \
         add the folder containing epistola.exe to your PATH manually."
            .to_string(),
    )
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn locate_cli_binary() -> Result<PathBuf, String> {
    let exe =
        std::env::current_exe().map_err(|err| format!("Couldn't locate the running app: {err}"))?;
    let dir = exe
        .parent()
        .ok_or_else(|| "The running app has no parent directory.".to_string())?;
    let candidate = dir.join("epistola");
    if candidate.is_file() {
        Ok(candidate)
    } else {
        Err(
            "No `epistola` CLI binary next to this app — this build may not include it."
                .to_string(),
        )
    }
}

#[cfg(target_os = "macos")]
fn install_symlink(binary: &Path) -> Result<PathBuf, String> {
    use std::process::Command;

    let target = PathBuf::from("/usr/local/bin/epistola");
    let shell_cmd = format!(
        "mkdir -p /usr/local/bin && ln -sf {} {}",
        shell_quote(&binary.to_string_lossy()),
        shell_quote(&target.to_string_lossy())
    );
    let script = format!(
        "do shell script \"{}\" with administrator privileges",
        applescript_quote(&shell_cmd)
    );
    let output = Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .map_err(|err| format!("Couldn't run osascript: {err}"))?;
    if output.status.success() {
        Ok(target)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!(
            "Installation was cancelled or failed: {}",
            stderr.trim()
        ))
    }
}

#[cfg(target_os = "macos")]
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

#[cfg(target_os = "macos")]
fn applescript_quote(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(target_os = "linux")]
fn install_symlink(binary: &Path) -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").ok_or_else(|| "HOME is not set.".to_string())?;
    let bin_dir = PathBuf::from(home).join(".local/bin");
    std::fs::create_dir_all(&bin_dir)
        .map_err(|err| format!("Couldn't create {}: {err}", bin_dir.display()))?;
    let target = bin_dir.join("epistola");
    let _ = std::fs::remove_file(&target);
    std::os::unix::fs::symlink(binary, &target)
        .map_err(|err| format!("Couldn't symlink {}: {err}", target.display()))?;
    Ok(target)
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn shell_quote_neutralizes_embedded_single_quotes() {
        let quoted = shell_quote("/tmp/it's a path/epistola");
        assert_eq!(quoted, r#"'/tmp/it'\''s a path/epistola'"#);
    }

    #[test]
    fn applescript_quote_escapes_backslashes_and_double_quotes() {
        let quoted = applescript_quote(r#"say "hi" \ done"#);
        assert_eq!(quoted, r#"say \"hi\" \\ done"#);
    }
}
