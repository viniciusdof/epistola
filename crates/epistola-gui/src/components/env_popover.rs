use gpui::{div, prelude::*, px, App, ClickEvent, FocusHandle, IntoElement, SharedString, Window};

use crate::theme::Theme;

pub fn render_env_popover(
    environments: &[String],
    current: Option<&str>,
    selected: usize,
    theme: Theme,
    focus_handle: &FocusHandle,
    on_select: impl Fn(String, &mut Window, &mut App) + Clone + 'static,
    on_dismiss: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id("env-popover-backdrop")
        .track_focus(focus_handle)
        .absolute()
        .inset_0()
        .flex()
        .justify_end()
        .items_start()
        .pt(px(44.))
        .pr(px(160.))
        .on_click(on_dismiss)
        .child(
            div()
                .id("env-popover-box")
                .w(px(230.))
                .bg(theme.surface_raised)
                .border_1()
                .border_color(theme.border)
                .rounded(px(8.))
                .shadow_lg()
                .p(px(5.))
                .on_click(|_event, _window, cx| cx.stop_propagation())
                .child(
                    div()
                        .px(px(8.))
                        .pt(px(6.))
                        .pb(px(4.))
                        .text_size(px(10.))
                        .text_color(theme.text_faint)
                        .child("Environments"),
                )
                .children(environments.iter().enumerate().map(|(i, env)| {
                    let active = current == Some(env.as_str());
                    let name = env.clone();
                    let on_select = on_select.clone();
                    div()
                        .id(SharedString::from(format!("env-popover-{env}")))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .px(px(8.))
                        .py(px(7.))
                        .rounded(px(5.))
                        .text_size(px(12.5))
                        .text_color(theme.text)
                        .cursor_pointer()
                        .when(i == selected, |el| el.bg(theme.surface))
                        .hover(|el| el.bg(theme.surface))
                        .on_click(move |_event, window, cx| {
                            cx.stop_propagation();
                            on_select(name.clone(), window, cx);
                        })
                        .child(div().flex_none().w(px(6.)).h(px(6.)).rounded(px(3.)).bg(
                            if active {
                                theme.success
                            } else {
                                theme.text_faint
                            },
                        ))
                        .child(div().flex_1().child(env.clone()))
                        .child(
                            div()
                                .text_size(px(10.))
                                .text_color(theme.text_faint)
                                .font_family("monospace")
                                .child(format!("{env}.toml")),
                        )
                }))
                .child(
                    div()
                        .border_t_1()
                        .border_color(theme.border)
                        .mt(px(4.))
                        .px(px(8.))
                        .pt(px(7.))
                        .pb(px(3.))
                        .text_size(px(10.5))
                        .text_color(theme.text_faint)
                        .child("Secrets in *.secrets.toml are merged in, never shown here."),
                ),
        )
}
