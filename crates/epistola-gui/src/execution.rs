//! Bridges GPUI's background executor to the Tokio reactor that
//! `epistola-engine`'s network calls need. GPUI owns `main`, so

use std::path::PathBuf;

use epistola_core::InterpolationError;
use epistola_engine::EngineError;
use epistola_format::FormatError;
use gpui::{Context, WeakEntity};

use crate::root::EpistolaGui;
use crate::state::{ActiveFile, ActivityResult};

pub(crate) fn classify_engine_error(err: EngineError) -> ActivityResult {
    if let EngineError::Format(format_err) = &err {
        if let FormatError::Interpolation(InterpolationError::UnknownVariable(variable)) =
            format_err.as_ref()
        {
            return ActivityResult::UnresolvedVariable {
                variable: variable.clone(),
            };
        }
    }
    ActivityResult::RunFailed(err.to_string())
}

pub fn spawn_run(path: PathBuf, environment: Option<String>, cx: &mut Context<EpistolaGui>) {
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
                )
                .await
            }))
            .await;

        let activity = match outcome {
            Ok(outcome) => ActivityResult::RunSuccess {
                status: outcome.response.status,
                duration_ms: outcome.response.duration.as_millis(),
                content_length: outcome.response.body.len(),
                body: String::from_utf8_lossy(&outcome.response.body).into_owned(),
                headers: outcome
                    .response
                    .headers
                    .iter()
                    .map(|header| (header.name.clone(), header.value.clone()))
                    .collect(),
            },
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
