//! Bridges GPUI's background executor to the Tokio reactor that
//! `epistola-engine`'s network calls need. GPUI owns `main`, so

use std::path::PathBuf;
use std::sync::Arc;

use epistola_core::InterpolationError;
use epistola_engine::{CookieJar, EngineError};
use epistola_format::FormatError;
use gpui::{Context, WeakEntity};

use crate::root::EpistolaGui;
use crate::state::{ActiveFile, ActivityResult};

pub(crate) fn unresolved_variable(err: &EngineError) -> Option<String> {
    if let EngineError::Format(format_err) = err {
        if let FormatError::Interpolation(InterpolationError::UnknownVariable(variable)) =
            format_err.as_ref()
        {
            return Some(variable.clone());
        }
    }
    None
}

pub(crate) fn classify_engine_error(err: EngineError) -> ActivityResult {
    if let Some(variable) = unresolved_variable(&err) {
        return ActivityResult::UnresolvedVariable { variable };
    }
    ActivityResult::RunFailed(err.to_string())
}

pub fn spawn_run(
    path: PathBuf,
    environment: Option<String>,
    cookie_jar: Arc<CookieJar>,
    cx: &mut Context<EpistolaGui>,
) {
    let tab = ActiveFile::Request(path.clone());
    cx.spawn(async move |weak: WeakEntity<EpistolaGui>, cx| {
        let _ = weak.update(cx, |this, cx| {
            this.state
                .activity
                .insert(tab.clone(), ActivityResult::Running);
            cx.notify();
        });

        let outcome = cx
            .background_executor()
            .spawn(async_compat::Compat::new(async move {
                let (collection, resolved) = epistola_engine::run::resolve_saved_request(
                    &path,
                    environment.as_deref(),
                    Default::default(),
                )?;
                epistola_engine::run::execute_and_log(
                    &resolved.request,
                    resolved.history_enabled,
                    &collection.root,
                    &epistola_engine::client::ClientOverrides::default(),
                    &collection.manifest.client,
                    Some(cookie_jar),
                )
                .await
            }))
            .await;

        let activity = match outcome {
            Ok(outcome) => ActivityResult::RunSuccess(outcome.response),
            Err(engine_err) => classify_engine_error(engine_err),
        };

        let _ = weak.update(cx, |this, cx| {
            this.state.activity.insert(tab.clone(), activity);
            cx.notify();
        });
    })
    .detach();
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use epistola_core::ExecutorError;

    use super::*;

    #[test]
    fn classify_engine_error_recognizes_an_unknown_variable() {
        let err = EngineError::Format(Box::new(FormatError::Interpolation(
            InterpolationError::UnknownVariable("user_id".to_string()),
        )));

        let activity = classify_engine_error(err);

        assert!(matches!(
            activity,
            ActivityResult::UnresolvedVariable { ref variable } if variable == "user_id"
        ));
    }

    #[test]
    fn classify_engine_error_falls_back_to_run_failed() {
        let err = EngineError::Executor(ExecutorError::InvalidRequest("bad url".to_string()));

        let activity = classify_engine_error(err);

        assert!(matches!(activity, ActivityResult::RunFailed(_)));
    }
}
