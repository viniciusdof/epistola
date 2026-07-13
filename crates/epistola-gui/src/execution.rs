//! Bridges GPUI's own foreground executor to the Tokio runtime that
//! `epistola-engine`'s network calls need. GPUI owns `main`, so
//! `#[tokio::main]` isn't an option — a `Runtime` is built once in `main()`
//! and stashed here instead.

use std::path::PathBuf;
use std::sync::OnceLock;

use gpui::{Context, WeakEntity};
use tokio::runtime::Runtime;

use crate::root::EpistolaGui;
use crate::state::ActivityResult;

static TOKIO: OnceLock<Runtime> = OnceLock::new();

pub fn install(runtime: Runtime) {
    let _ = TOKIO.set(runtime);
}

fn handle() -> Option<tokio::runtime::Handle> {
    TOKIO.get().map(Runtime::handle).cloned()
}

pub fn spawn_run(path: PathBuf, environment: Option<String>, cx: &mut Context<EpistolaGui>) {
    cx.spawn(async move |weak: WeakEntity<EpistolaGui>, cx| {
        let _ = weak.update(cx, |this, cx| {
            this.state.activity = ActivityResult::Running;
            cx.notify();
        });

        let Some(handle) = handle() else {
            let _ = weak.update(cx, |this, cx| {
                this.state.activity =
                    ActivityResult::RunFailed("no async runtime available".into());
                cx.notify();
            });
            return;
        };

        let outcome = handle
            .spawn(async move {
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
            })
            .await;

        let activity = match outcome {
            Ok(Ok(outcome)) => ActivityResult::RunSuccess {
                status: outcome.response.status,
                duration_ms: outcome.response.duration.as_millis(),
                content_length: outcome.response.body.len(),
                body: String::from_utf8_lossy(&outcome.response.body).into_owned(),
            },
            Ok(Err(engine_err)) => ActivityResult::RunFailed(engine_err.to_string()),
            Err(join_err) => ActivityResult::RunFailed(join_err.to_string()),
        };

        let _ = weak.update(cx, |this, cx| {
            this.state.activity = activity;
            cx.notify();
        });
    })
    .detach();
}
