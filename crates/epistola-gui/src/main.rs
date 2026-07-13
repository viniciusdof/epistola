mod actions;
mod assets;
mod collection;
mod components;
mod execution;
mod root;
mod state;
mod theme;

use std::env;

use gpui::{
    point, prelude::*, px, size, App, Bounds, TitlebarOptions, WindowBounds, WindowOptions,
};

use assets::Assets;
use root::EpistolaGui;

fn main() {
    match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => execution::install(runtime),
        Err(err) => eprintln!("failed to start async runtime, requests won't be sendable: {err}"),
    }

    let cwd = env::current_dir().unwrap_or_default();

    gpui_platform::application()
        .with_assets(Assets)
        .run(move |cx: &mut App| {
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
                |_, cx| cx.new(|_| EpistolaGui::new(cwd.clone())),
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
