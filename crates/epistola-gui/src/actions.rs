use gpui::{actions, Context, PathPromptOptions};

use crate::root::EpistolaGui;
use crate::state::{ConfirmDiscardKind, Overlay};

actions!(
    editor,
    [
        Backspace,
        Delete,
        InsertNewline,
        MoveLeft,
        MoveRight,
        MoveUp,
        MoveDown,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectAll,
        Home,
        End,
        Paste,
        Cut,
        Copy,
        Save,
    ]
);

actions!(
    app,
    [
        ToggleCommandPalette,
        ToggleQuickOpen,
        OpenSettings,
        GoHome,
        RunActiveRequest,
        ShowResolvedRequest,
        LintCollection,
        CycleEnvironment,
        OpenCollection,
        Dismiss,
    ]
);

fn folder_prompt(prompt: &str) -> PathPromptOptions {
    PathPromptOptions {
        files: false,
        directories: true,
        multiple: false,
        prompt: Some(prompt.into()),
    }
}

pub fn spawn_open_collection(cx: &mut Context<EpistolaGui>) {
    let prompt = cx.prompt_for_paths(folder_prompt("Open"));
    cx.spawn(async move |weak, cx| {
        let Ok(Ok(Some(mut paths))) = prompt.await else {
            return;
        };
        let Some(path) = paths.pop() else {
            return;
        };
        let _ = weak.update(cx, |this, cx| {
            if this.state.has_unsaved_changes() {
                this.state.overlay = Some(Overlay::ConfirmDiscard(
                    ConfirmDiscardKind::SwitchCollection(path),
                ));
            } else {
                this.state.open_collection_at(path);
            }
            cx.notify();
        });
    })
    .detach();
}

/// The collection's name defaults to the chosen folder's name
pub fn spawn_new_collection(cx: &mut Context<EpistolaGui>) {
    let prompt = cx.prompt_for_paths(folder_prompt("New Collection"));
    cx.spawn(async move |weak, cx| {
        let Ok(Ok(Some(mut paths))) = prompt.await else {
            return;
        };
        let Some(path) = paths.pop() else {
            return;
        };
        let outcome = epistola_engine::init::init_collection(&path, None, None);
        let _ = weak.update(cx, |this, cx| {
            match outcome {
                Ok(outcome) => {
                    if this.state.has_unsaved_changes() {
                        this.state.overlay = Some(Overlay::ConfirmDiscard(
                            ConfirmDiscardKind::SwitchCollection(outcome.path),
                        ));
                    } else {
                        this.state.open_collection_at(outcome.path);
                    }
                }
                Err(err) => this.state.collection_action_error = Some(err.to_string()),
            }
            cx.notify();
        });
    })
    .detach();
}
