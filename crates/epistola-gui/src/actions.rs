use std::path::PathBuf;

use gpui::{actions, Action, Context, PathPromptOptions};

use crate::root::EpistolaGui;
use crate::state::{ActiveFile, ResponseSubTab};

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
        Undo,
        Redo,
    ]
);

actions!(
    app,
    [
        ToggleCommandPalette,
        ToggleQuickOpen,
        OpenSettings,
        GoHome,
        GoWorkspace,
        RunActiveRequest,
        ShowResolvedRequest,
        LintCollection,
        CycleEnvironment,
        OpenCollection,
        NewCollection,
        OpenEnvironmentPicker,
        OpenHistory,
        NewRequest,
        RenameRequest,
        DuplicateRequest,
        DeleteRequest,
        Dismiss,
        ToggleSidebar,
        ToggleDrawer,
    ]
);

/// Navigation/CRUD actions with payload: unlike the unit actions above these
/// carry the data a click site already has (a path, a name, a tab) so the
/// same dispatch works whether it comes from a keybinding, the palette, or a
/// mouse click. None are ever built from a JSON keymap, hence `no_json`.
#[derive(Clone, PartialEq, Action)]
#[action(namespace = app, no_json)]
pub struct OpenRequestFile {
    pub path: PathBuf,
}

#[derive(Clone, PartialEq, Action)]
#[action(namespace = app, no_json)]
pub struct OpenFolderDoc {
    pub dir: PathBuf,
}

#[derive(Clone, PartialEq, Action)]
#[action(namespace = app, no_json)]
pub struct OpenEnvironmentDoc {
    pub name: String,
}

#[derive(Clone, PartialEq, Action)]
#[action(namespace = app, no_json)]
pub struct SwitchTab {
    pub file: ActiveFile,
}

#[derive(Clone, PartialEq, Action)]
#[action(namespace = app, no_json)]
pub struct CloseTab {
    pub file: ActiveFile,
}

#[derive(Clone, PartialEq, Action)]
#[action(namespace = app, no_json)]
pub struct SelectEnvironment {
    pub name: String,
}

#[derive(Clone, PartialEq, Action)]
#[action(namespace = app, no_json)]
pub struct OpenRecentCollection {
    pub path: PathBuf,
}

#[derive(Clone, PartialEq, Action)]
#[action(namespace = app, no_json)]
pub struct SelectResponseSubtab {
    pub subtab: ResponseSubTab,
}

#[derive(Clone, PartialEq, Action)]
#[action(namespace = app, no_json)]
pub struct ToggleFolderCollapse {
    pub dir: PathBuf,
}

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
        let _ = weak.update_in(cx, |this, window, cx| {
            this.switch_collection_with_confirm(path, window, cx);
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
        match outcome {
            Ok(outcome) => {
                let _ = weak.update_in(cx, |this, window, cx| {
                    this.switch_collection_with_confirm(outcome.path, window, cx);
                });
            }
            Err(err) => {
                let _ = weak.update(cx, |this, cx| {
                    this.state.collection_action_error = Some(err.to_string());
                    cx.notify();
                });
            }
        }
    })
    .detach();
}
