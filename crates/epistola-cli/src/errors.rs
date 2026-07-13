/// Errors specific to the CLI layer, distinguished in `main`'s exit-code
/// mapping (see `main::exit_code_for`) from `epistola-core`/`epistola-format`
/// errors that already flow through as `anyhow` contexts.
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("HTTP status {0} indicates failure (--check-status)")]
    HttpStatus(u16),
    #[error("{0} request file(s) failed lint")]
    LintFailed(usize),
}
