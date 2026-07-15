mod actions;
mod assets;
mod buffer;
mod collection;
mod components;
mod editor_actions;
mod editor_input;
mod editor_save;
mod execution;
mod root;
mod state;
mod theme;

use std::env;

use gpui::{
    point, prelude::*, px, size, App, Bounds, KeyBinding, TitlebarOptions, WindowBounds,
    WindowOptions,
};

use actions::{
    Backspace, Copy, Cut, CycleEnvironment, Delete, Dismiss, End, GoHome, Home, InsertNewline,
    LintCollection, MoveDown, MoveLeft, MoveRight, MoveUp, OpenCollection, OpenSettings, Paste,
    RunActiveRequest, Save, SelectAll, SelectDown, SelectLeft, SelectRight, SelectUp,
    ShowResolvedRequest, ToggleCommandPalette, ToggleQuickOpen,
};
use assets::Assets;
use root::EpistolaGui;
use theme::Theme;

fn main() {
    let cwd = env::current_dir().unwrap_or_default();

    gpui_platform::application()
        .with_assets(Assets)
        .run(move |cx: &mut App| {
            cx.set_global(Theme::dark());

            cx.bind_keys([
                KeyBinding::new("backspace", Backspace, Some("Editor")),
                KeyBinding::new("delete", Delete, Some("Editor")),
                KeyBinding::new("enter", InsertNewline, Some("Editor")),
                KeyBinding::new("left", MoveLeft, Some("Editor")),
                KeyBinding::new("right", MoveRight, Some("Editor")),
                KeyBinding::new("up", MoveUp, Some("Editor")),
                KeyBinding::new("down", MoveDown, Some("Editor")),
                KeyBinding::new("shift-left", SelectLeft, Some("Editor")),
                KeyBinding::new("shift-right", SelectRight, Some("Editor")),
                KeyBinding::new("shift-up", SelectUp, Some("Editor")),
                KeyBinding::new("shift-down", SelectDown, Some("Editor")),
                KeyBinding::new("cmd-a", SelectAll, Some("Editor")),
                KeyBinding::new("home", Home, Some("Editor")),
                KeyBinding::new("end", End, Some("Editor")),
                KeyBinding::new("cmd-v", Paste, Some("Editor")),
                KeyBinding::new("cmd-c", Copy, Some("Editor")),
                KeyBinding::new("cmd-x", Cut, Some("Editor")),
                KeyBinding::new("cmd-s", Save, Some("Editor")),
                KeyBinding::new("cmd-k", ToggleCommandPalette, None),
                KeyBinding::new("cmd-p", ToggleQuickOpen, None),
                KeyBinding::new("cmd-,", OpenSettings, None),
                KeyBinding::new("cmd-0", GoHome, None),
                KeyBinding::new("cmd-enter", RunActiveRequest, None),
                KeyBinding::new("cmd-shift-r", ShowResolvedRequest, None),
                KeyBinding::new("cmd-shift-l", LintCollection, None),
                KeyBinding::new("cmd-e", CycleEnvironment, None),
                KeyBinding::new("cmd-o", OpenCollection, None),
                KeyBinding::new("escape", Dismiss, None),
            ]);

            let bounds = Bounds::centered(None, size(px(1080.0), px(700.0)), cx);
            let window = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: None,
                        appears_transparent: true,
                        traffic_light_position: Some(point(px(9.), px(9.))),
                    }),
                    ..Default::default()
                },
                |_, cx| cx.new(|cx| EpistolaGui::new(cwd.clone(), cx)),
            );

            match window {
                Ok(_) => cx.activate(true),
                Err(err) => {
                    eprintln!("failed to open window: {err}");
                    cx.quit();
                }
            }
        });
}
