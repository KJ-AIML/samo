//! Pop-ups: a short note under Samo's menu bar item, such as "a Claude Code session needs you".
//!
//! The window is a non-activating panel (`WindowKind::PopUp`) opened with `focus: false`, so it never takes the
//! keyboard from the app you are typing in. One shows at a time: a newer pop-up replaces the one showing, and it
//! hides itself after a few seconds. A click opens Samo on the page it is about.

use crate::components::*;
use crate::theme::*;
use crate::{AppState, NavSection, open_main};
use gpui::{
    App, Bounds, Context, FontWeight, IntoElement, MouseButton, ParentElement, Render, Styled,
    Window, WindowAppearance, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions,
    div, point, prelude::*, px, size,
};
use std::time::Duration;

pub const POPUP_W: f32 = 300.0;
pub const POPUP_H: f32 = 72.0;
/// How long a pop-up stays before it hides itself.
const SHOWN_FOR: Duration = Duration::from_secs(6);
/// The closest a pop-up comes to the screen's edges.
const MARGIN: f32 = 8.0;
/// Without the menu bar item's position, a pop-up goes at the right, just under the menu bar.
const FALLBACK_TOP: f32 = 32.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PopupKind {
    /// Something is waiting for you.
    NeedsYou,
    /// Something you started has finished.
    Finished,
    Info,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Popup {
    pub kind: PopupKind,
    pub title: String,
    pub body: String,
    /// The page a click opens; `None` opens Samo where it was.
    pub target: Option<NavSection>,
}

/// The menu bar item's rectangle in screen pixels, as `tray-icon` reports it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrayRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Where a pop-up's top-left corner goes, in points: centred under the menu bar item and kept on screen, or at
/// the top right when the item's position isn't known.
pub fn origin(tray: Option<TrayRect>, scale: f64, display_w: f32) -> (f32, f32) {
    let rightmost = (display_w - POPUP_W - MARGIN).max(MARGIN);
    match tray {
        Some(r) if scale > 0.0 => {
            let centre = ((r.x + r.w / 2.0) / scale) as f32;
            let bottom = ((r.y + r.h) / scale) as f32;
            (
                (centre - POPUP_W / 2.0).clamp(MARGIN, rightmost),
                bottom + 6.0,
            )
        }
        _ => (rightmost, FALLBACK_TOP),
    }
}

/// Shows `popup`, replacing the one showing if there is one, and hides it after a few seconds.
pub fn show(popup: Popup, cx: &mut App) {
    let generation = cx.global::<AppState>().popup_gen.wrapping_add(1);
    cx.global_mut::<AppState>().popup_gen = generation;

    let replaced = cx.global::<AppState>().popup.is_some_and(|handle| {
        let next = popup.clone();
        handle
            .update(cx, |view, _, cx| {
                view.popup = next;
                cx.notify();
            })
            .is_ok()
    });
    if !replaced {
        let tray = cx
            .global::<AppState>()
            .menubar
            .as_ref()
            .and_then(|m| m.tray_rect());
        let display_w = cx
            .primary_display()
            .map(|d| f32::from(d.bounds().size.width))
            .unwrap_or(1440.0);
        let (x, y) = origin(tray, crate::menubar::main_screen_scale(), display_w);
        let opened = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(x), px(y)),
                    size: size(px(POPUP_W), px(POPUP_H)),
                })),
                titlebar: None,
                // Never take the keyboard: the panel doesn't activate Samo and isn't made key.
                focus: false,
                show: true,
                kind: WindowKind::PopUp,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent,
                ..Default::default()
            },
            |_, cx| cx.new(|_| PopupView { popup }),
        );
        match opened {
            Ok(handle) => cx.global_mut::<AppState>().popup = Some(handle),
            Err(_) => return,
        }
    }

    cx.spawn(async move |cx| {
        cx.background_executor().timer(SHOWN_FOR).await;
        let _ = cx.update(|cx| {
            if cx.global::<AppState>().popup_gen == generation {
                hide(cx);
            }
        });
    })
    .detach();
}

/// Hides the pop-up showing, if any.
pub fn hide(cx: &mut App) {
    if let Some(handle) = cx.global_mut::<AppState>().popup.take() {
        let _ = handle.update(cx, |_, window, _| window.remove_window());
    }
}

pub struct PopupView {
    popup: Popup,
}

impl Render for PopupView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        set_dark(pinned_dark().unwrap_or_else(|| {
            matches!(
                window.appearance(),
                WindowAppearance::Dark | WindowAppearance::VibrantDark
            )
        }));
        let pal = p();
        let glyph = match self.popup.kind {
            PopupKind::NeedsYou => "!",
            PopupKind::Finished => "v",
            PopupKind::Info => "i",
        };
        div()
            .size_full()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(S3))
            .px(px(14.0))
            .rounded(px(R_CARD))
            .bg(pal.frame)
            .border_1()
            .border_color(pal.hairline_strong)
            .overflow_hidden()
            .text_color(pal.text)
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    let target = this.popup.target;
                    hide(cx);
                    open_main(cx, target);
                }),
            )
            // The glyph sits in a ring, Samo's shape.
            .child(
                div()
                    .size(px(30.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .border_2()
                    .border_color(pal.text)
                    .font(mono(FontWeight::BOLD))
                    .text_size(px(T_LEAD))
                    .child(glyph),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        line(
                            self.popup.title.clone(),
                            FontWeight::MEDIUM,
                            T_BODY,
                            pal.text,
                        )
                        .w_full(),
                    )
                    .child(
                        line(
                            self.popup.body.clone(),
                            FontWeight::NORMAL,
                            T_META,
                            pal.text_2,
                        )
                        .w_full(),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_under_tray_and_clamped_to_screen() {
        // A 2x screen: the item at x 2000..2060 px, y 0..48 px, so its centre is at 1015 pt.
        let tray = TrayRect {
            x: 2000.0,
            y: 0.0,
            w: 60.0,
            h: 48.0,
        };
        let (x, y) = origin(Some(tray), 2.0, 1512.0);
        assert!((x - (1015.0 - POPUP_W / 2.0)).abs() < 0.01);
        assert!((y - 30.0).abs() < 0.01);

        // An item at the far right edge: the pop-up stays on screen.
        let far = TrayRect {
            x: 3000.0,
            y: 0.0,
            w: 40.0,
            h: 48.0,
        };
        let (x, _) = origin(Some(far), 2.0, 1512.0);
        assert!((x - (1512.0 - POPUP_W - MARGIN)).abs() < 0.01);

        // And at the far left.
        let left = TrayRect {
            x: 0.0,
            y: 0.0,
            w: 40.0,
            h: 48.0,
        };
        let (x, _) = origin(Some(left), 2.0, 1512.0);
        assert!((x - MARGIN).abs() < 0.01);
    }

    #[test]
    fn falls_back_to_top_right_without_tray_rect() {
        let (x, y) = origin(None, 2.0, 1512.0);
        assert!((x - (1512.0 - POPUP_W - MARGIN)).abs() < 0.01);
        assert!((y - FALLBACK_TOP).abs() < 0.01);

        // A scale of zero is no position either.
        let tray = TrayRect {
            x: 10.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        };
        assert_eq!(origin(Some(tray), 0.0, 1512.0), origin(None, 0.0, 1512.0));
    }
}
