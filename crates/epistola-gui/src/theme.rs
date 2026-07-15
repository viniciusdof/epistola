//! Central design tokens for the GUI. Components read colors from a
//! `Theme` value instead of hard-coding hex literals.

use epistola_core::Method;
use gpui::{rgb, Hsla};

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub ink: Hsla,
    pub surface: Hsla,
    pub surface_raised: Hsla,
    pub border: Hsla,
    pub text: Hsla,
    pub text_muted: Hsla,
    pub text_faint: Hsla,
    pub accent: Hsla,
    pub accent_ink: Hsla,
    pub method_get: Hsla,
    pub method_post: Hsla,
    pub method_put: Hsla,
    pub method_delete: Hsla,
    pub success: Hsla,
}

impl gpui::Global for Theme {}

impl Theme {
    pub fn dark() -> Self {
        Self {
            ink: rgb(0x191a22).into(),
            surface: rgb(0x20212c).into(),
            surface_raised: rgb(0x272839).into(),
            border: rgb(0x34364a).into(),
            text: rgb(0xe7e5ee).into(),
            text_muted: rgb(0x8d8ba3).into(),
            text_faint: rgb(0x55536c).into(),
            accent: rgb(0xe0a030).into(),
            accent_ink: rgb(0x2b2107).into(),
            method_get: rgb(0x5fb894).into(),
            method_post: rgb(0xe0a030).into(),
            method_put: rgb(0x9d8cf0).into(),
            method_delete: rgb(0xd1614a).into(),
            success: rgb(0x5fb894).into(),
        }
    }

    pub fn method_color(&self, method: &Method) -> Hsla {
        match method {
            Method::Get => self.method_get,
            Method::Post => self.method_post,
            Method::Put | Method::Patch => self.method_put,
            Method::Delete => self.method_delete,
            Method::Head | Method::Options | Method::Query | Method::Other(_) => self.text_muted,
        }
    }
}
