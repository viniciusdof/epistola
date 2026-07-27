use std::path::PathBuf;

use gpui::{Context, PromptLevel, Window};

use crate::collection;
use crate::execution;
use crate::root::EpistolaGui;
use crate::state::{ActiveFile, ActivityResult, Overlay, PromptKind};

impl EpistolaGui {
    pub(crate) fn open_settings(&mut self, cx: &mut Context<Self>) {
        self.state.open_config();
        self.sync_editor_view(cx);
        cx.notify();
    }

    pub(crate) fn run_active_request(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.state.active_request().map(|r| r.abs_path.clone()) else {
            return;
        };
        execution::spawn_run(
            path,
            self.state.environment.clone(),
            self.state.cookie_jar.clone(),
            cx,
        );
    }

    pub(crate) fn show_resolved_request(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.state.active_request().map(|r| r.abs_path.clone()) else {
            return;
        };
        let tab = self.state.active_file.clone();
        let outcome = epistola_engine::run::resolve_saved_request(
            &path,
            self.state.environment.as_deref(),
            Default::default(),
        );
        let activity = match outcome {
            Ok((_collection, resolved)) => ActivityResult::Resolved(
                epistola_engine::output::format_request_text(&resolved.request),
            ),
            Err(engine_err) => match execution::classify_engine_error(engine_err) {
                ActivityResult::UnresolvedVariable { variable } => {
                    ActivityResult::UnresolvedVariable { variable }
                }
                ActivityResult::RunFailed(message) => ActivityResult::ResolvedFailed(message),
                other => other,
            },
        };
        self.state.activity.insert(tab, activity);
        self.sync_response_view(cx);
        cx.notify();
    }

    pub(crate) fn lint_collection(&mut self, cx: &mut Context<Self>) {
        if self.state.collection().is_none() {
            return;
        }
        let tab = self.state.active_file.clone();
        let result = epistola_engine::discovery::discover_collection(&self.state.cwd)
            .and_then(|collection| {
                epistola_engine::requests::lint_collection(
                    &collection,
                    self.state.environment.as_deref(),
                )
            })
            .map(|report| format_lint_report(&report))
            .map_err(|err| err.to_string());
        let activity = match result {
            Ok(text) => ActivityResult::Linted(text),
            Err(err) => ActivityResult::LintFailed(err),
        };
        self.state.activity.insert(tab, activity);
        self.sync_response_view(cx);
        cx.notify();
    }

    pub(crate) fn cycle_environment_action(&mut self, cx: &mut Context<Self>) {
        if self.state.collection().is_none() {
            return;
        }
        self.state.cycle_environment();
        cx.notify();
    }

    pub(crate) fn clear_cookie_jar(&mut self, cx: &mut Context<Self>) {
        self.state.cookie_jar = std::sync::Arc::new(epistola_engine::CookieJar::default());
        cx.notify();
    }

    /// Relative to the collection root: the directory of the request or
    /// folder currently open, or the root itself if neither is open.
    fn default_new_request_dir(&self) -> String {
        let Some(collection) = self.state.collection() else {
            return String::new();
        };
        let dir = match &self.state.active_file {
            ActiveFile::Request(path) => path.parent().map(|p| p.to_path_buf()),
            ActiveFile::Folder(dir) => Some(dir.clone()),
            _ => None,
        };
        dir.and_then(|d| {
            d.strip_prefix(&collection.root)
                .ok()
                .map(|p| p.to_string_lossy().into_owned())
        })
        .unwrap_or_default()
    }

    pub(crate) fn start_new_request_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dir = self.default_new_request_dir();
        self.open_text_overlay(
            Overlay::Prompt(PromptKind::New { dir }),
            "Request name…",
            window,
            cx,
        );
    }

    pub(crate) fn start_new_folder_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dir = self.default_new_request_dir();
        self.open_text_overlay(
            Overlay::Prompt(PromptKind::NewFolder { dir }),
            "Folder name…",
            window,
            cx,
        );
    }

    pub(crate) fn start_new_environment_prompt(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_text_overlay(
            Overlay::Prompt(PromptKind::NewEnvironment),
            "Environment name…",
            window,
            cx,
        );
    }

    pub(crate) fn start_rename_request_prompt(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((path, name)) = self
            .state
            .active_request()
            .map(|r| (r.abs_path.clone(), r.display_name.clone()))
        else {
            return;
        };
        self.open_text_overlay(
            Overlay::Prompt(PromptKind::Rename { path }),
            "Request name…",
            window,
            cx,
        );
        self.overlay_input
            .update(cx, |field, cx| field.set_text(name, cx));
    }

    pub(crate) fn start_duplicate_request_prompt(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((path, suggested)) = self
            .state
            .active_request()
            .map(|r| (r.abs_path.clone(), format!("{} copy", r.display_name)))
        else {
            return;
        };
        self.open_text_overlay(
            Overlay::Prompt(PromptKind::Duplicate { path }),
            "Request name…",
            window,
            cx,
        );
        self.overlay_input
            .update(cx, |field, cx| field.set_text(suggested, cx));
    }

    pub(crate) fn start_delete_request_confirm(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self.state.active_request().map(|r| r.abs_path.clone()) else {
            return;
        };
        self.delete_request_with_confirm(path, window, cx);
    }

    /// Shows a `window.prompt` and, if the user picked an option (any index,
    /// i.e. didn't dismiss it), calls `on_confirm` with the picked index.
    /// Shared skeleton for the three "are you sure?" flows below — each
    /// still owns its own message/options and what each index means.
    fn confirm_and_then(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        message: &str,
        detail: &str,
        options: &[&str],
        on_confirm: impl FnOnce(usize, &mut Self, &mut Context<Self>) + 'static,
    ) {
        let answer = window.prompt(PromptLevel::Warning, message, Some(detail), options, cx);
        cx.spawn(async move |weak, cx| {
            if let Ok(idx) = answer.await {
                let _ = weak.update(cx, |this, cx| {
                    on_confirm(idx, this, cx);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    pub(crate) fn delete_request_with_confirm(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = self
            .state
            .collection()
            .and_then(|c| c.find_request(&path))
            .map(|r| r.display_name.clone())
            .unwrap_or_else(|| path.display().to_string());
        self.confirm_and_then(
            window,
            cx,
            &format!("Delete request \"{name}\"?"),
            "This can't be undone.",
            &["Delete", "Cancel"],
            move |idx, this, cx| {
                if idx != 0 {
                    return;
                }
                match epistola_engine::requests::delete_request(&path) {
                    Ok(()) => {
                        collection::spawn_refresh_collection(this.state.cwd.clone(), cx);
                        this.finish_closing_tab(&ActiveFile::Request(path.clone()), cx);
                    }
                    Err(err) => {
                        this.state.collection_action_error = Some(err.to_string());
                    }
                }
            },
        );
    }

    pub(crate) fn submit_prompt(
        &mut self,
        kind: PromptKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = self.overlay_input.read(cx).text().trim().to_string();
        if name.is_empty() {
            self.state.overlay_error = Some("Name can't be empty".to_string());
            cx.notify();
            return;
        }

        let result = match &kind {
            PromptKind::New { dir } => {
                epistola_engine::requests::create_request(&self.state.cwd, dir, &name, "GET", "")
            }
            PromptKind::Rename { path } => epistola_engine::requests::rename_request(path, &name),
            PromptKind::Duplicate { path } => {
                epistola_engine::requests::duplicate_request(path, &name)
            }
            PromptKind::NewFolder { dir } => {
                let target = if dir.is_empty() {
                    name.clone()
                } else {
                    format!("{dir}/{name}")
                };
                epistola_engine::folder::init_folder(&self.state.cwd, &target)
            }
            PromptKind::NewEnvironment => {
                epistola_engine::discovery::discover_collection(&self.state.cwd).and_then(
                    |collection| epistola_engine::environments::new_environment(&collection, &name),
                )
            }
        };

        match result {
            Ok(new_path) => {
                collection::spawn_refresh_collection(self.state.cwd.clone(), cx);
                match &kind {
                    PromptKind::New { .. } | PromptKind::Duplicate { .. } => {
                        self.state.open_request(new_path);
                        self.sync_editor_view(cx);
                    }
                    PromptKind::Rename { path } => {
                        let old_file = ActiveFile::Request(path.clone());
                        let new_file = ActiveFile::Request(new_path.clone());
                        self.editor_view
                            .update(cx, |ev, _| ev.rename_buffer(&old_file, new_file));
                        self.state.replace_request_tab(path, new_path);
                        self.sync_editor_view(cx);
                    }
                    PromptKind::NewFolder { .. } => {
                        let folder_dir = new_path
                            .parent()
                            .map(|p| p.to_path_buf())
                            .unwrap_or(new_path);
                        self.state.open_folder_doc(folder_dir);
                        self.sync_editor_view(cx);
                    }
                    PromptKind::NewEnvironment => {
                        self.state.open_environment_doc(name.clone());
                        self.sync_editor_view(cx);
                    }
                }
                self.close_overlay(window, cx);
            }
            Err(err) => {
                self.state.overlay_error = Some(err.to_string());
                cx.notify();
            }
        }
    }

    pub(crate) fn switch_collection_with_confirm(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.editor_view.read(cx).has_unsaved_changes() {
            self.state.begin_open_collection(path.clone());
            self.editor_view.update(cx, |ev, _| ev.clear());
            collection::spawn_load_collection(path, cx);
            cx.notify();
            return;
        }
        self.confirm_and_then(
            window,
            cx,
            "This collection has unsaved changes in one or more tabs.",
            "Discard them and switch anyway?",
            &["Discard", "Cancel"],
            move |idx, this, cx| {
                if idx != 0 {
                    return;
                }
                this.state.begin_open_collection(path.clone());
                this.editor_view.update(cx, |ev, _| ev.clear());
                collection::spawn_load_collection(path, cx);
            },
        );
    }

    /// Removes `file`'s tab/buffer and re-syncs `EditorView` to whatever tab
    /// becomes active — the common tail of all three `close_tab_with_confirm`
    /// branches (clean-close, save-then-close, discard-then-close).
    pub(crate) fn finish_closing_tab(&mut self, file: &ActiveFile, cx: &mut Context<Self>) {
        self.state.close_tab(file);
        self.editor_view.update(cx, |ev, _| ev.remove_buffer(file));
        self.sync_editor_view(cx);
    }

    pub(crate) fn close_tab_with_confirm(
        &mut self,
        file: ActiveFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.editor_view.read(cx).is_dirty(&file) {
            self.finish_closing_tab(&file, cx);
            cx.notify();
            return;
        }
        self.confirm_and_then(
            window,
            cx,
            "This tab has unsaved changes.",
            "Save before closing?",
            &["Save", "Discard", "Cancel"],
            move |idx, this, cx| match idx {
                0 => {
                    this.editor_view
                        .update(cx, |ev, cx| ev.save_file(&file, cx));
                    if !this.editor_view.read(cx).is_dirty(&file) {
                        this.finish_closing_tab(&file, cx);
                    }
                }
                1 => this.finish_closing_tab(&file, cx),
                _ => {}
            },
        );
    }
}

fn format_lint_report(report: &epistola_engine::requests::LintReport) -> String {
    if report.issues.is_empty() {
        format!("Checked {} request(s) · 0 issues", report.checked)
    } else {
        let mut out = format!(
            "Checked {} request(s) · {} issue(s)\n",
            report.checked,
            report.issues.len()
        );
        for issue in &report.issues {
            out.push_str(&format!("  {}: {}\n", issue.path.display(), issue.message));
        }
        out
    }
}
