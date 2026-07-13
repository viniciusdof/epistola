use gpui::{
    div, prelude::*, px, rgb, size, App, Bounds, Context, Window, WindowBounds, WindowOptions,
};

struct EpistolaGui;

impl Render for EpistolaGui {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .size_full()
            .justify_center()
            .items_center()
            .bg(rgb(0x1e1e1e))
            .text_color(rgb(0xffffff))
            .text_xl()
            .child("epistola")
    }
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(800.0), px(600.0)), cx);
        let window = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| EpistolaGui),
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
