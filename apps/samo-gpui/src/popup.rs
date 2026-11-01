//! Pop-ups: a short note under Samo's menu bar item, such as "a Claude Code session needs you".
//!
//! The window is a non-activating panel (`WindowKind::PopUp`) opened with `focus: false`, so it never takes the
//! keyboard from the app you are typing in. One shows at a time: a newer pop-up replaces the one showing.
//!
//! It reads like the rest of Samo: a dial ring says what kind of news it is (an Ink `!` when something waits
//! for you, a Cobalt check when something finished), a mono line says where it's from and when, then the
//! title and one line. It arrives with a short drop and fade, stays 6 s (longer while the pointer is on it,
//! which also shows a close button) and fades out. A click opens Samo on the page it is about.

use crate::art::dial_chip;
use crate::components::*;
use crate::theme::*;
use crate::{AppState, NavSection, open_main};
use gpui::{
    Animation, AnimationExt, App, Bounds, Context, FontWeight, Hsla, IntoElement, MouseButton,
    MouseExitEvent, ParentElement, Render, Styled, Window, WindowAppearance,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, canvas, div,
    ease_out_quint, point, prelude::*, px, size,
};
use std::time::Duration;

/// The card's size, which is also the window's: macOS draws the shadow, following the card's rounded shape. (A
/// shadow drawn by Samo inside a larger transparent window was cut off at the window's edge and showed as a box.)
pub const POPUP_W: f32 = 340.0;
pub const POPUP_H: f32 = 84.0;
/// How long a pop-up stays before it fades, unless the pointer is on it.
const SHOWN_FOR: Duration = Duration::from_secs(6);
/// While the pointer is on it, it is looked at again this often…
const HOVER_RECHECK: Duration = Duration::from_millis(800);
/// …but never held longer than about 30 s, in case the pointer's leaving was missed.
const MAX_HOLDS: u32 = 40;
const ARRIVE: Duration = Duration::from_millis(220);
const LEAVE: Duration = Duration::from_millis(160);
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

impl PopupKind {
    /// The word on the pop-up's mono line.
    pub fn word(self) -> &'static str {
        match self {
            Self::NeedsYou => "needs you",
            Self::Finished => "finished",
            Self::Info => "note",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Popup {
    pub kind: PopupKind,
    /// Who it's from: an extension's name, or `Samo`.
    pub source: String,
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

/// Where the card's top-left corner goes, in points: centred under the menu bar item and kept on screen, or at
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

/// The mono line: `CLAUDE SESSIONS · FINISHED`.
pub fn header(source: &str, kind: PopupKind) -> String {
    if source.is_empty() {
        kind.word().to_uppercase()
    } else {
        format!("{} · {}", source, kind.word()).to_uppercase()
    }
}

fn now_hhmm() -> String {
    chrono::Local::now().format("%H:%M").to_string()
}

/// Shows `popup`, replacing the one showing if there is one, and fades it out after a few seconds.
pub fn show(popup: Popup, cx: &mut App) {
    let generation = cx.global::<AppState>().popup_gen.wrapping_add(1);
    cx.global_mut::<AppState>().popup_gen = generation;
    let at = now_hhmm();

    let replaced = cx.global::<AppState>().popup.is_some_and(|handle| {
        let next = popup.clone();
        let at = at.clone();
        handle
            .update(cx, |view, _, cx| {
                view.popup = next;
                view.at = at;
                view.leaving = false;
                view.serial = view.serial.wrapping_add(1);
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
            |_, cx| {
                cx.new(|_| PopupView {
                    popup,
                    at,
                    hovered: false,
                    leaving: false,
                    serial: 0,
                })
            },
        );
        match opened {
            Ok(handle) => cx.global_mut::<AppState>().popup = Some(handle),
            Err(_) => return,
        }
    }

    cx.spawn(async move |cx| {
        cx.background_executor().timer(SHOWN_FOR).await;
        let mut holds = 0u32;
        loop {
            holds += 1;
            // Still this pop-up? Being looked at (for a while at most)? Otherwise start fading.
            let next = cx.update(|cx| {
                let state = cx.global::<AppState>();
                if state.popup_gen != generation {
                    return None;
                }
                let handle = state.popup?;
                handle
                    .update(cx, |view, _, cx| {
                        if view.hovered && holds < MAX_HOLDS {
                            false
                        } else {
                            view.leaving = true;
                            cx.notify();
                            true
                        }
                    })
                    .ok()
            });
            match next {
                Ok(Some(true)) => {
                    cx.background_executor().timer(LEAVE).await;
                    let _ = cx.update(|cx| {
                        if cx.global::<AppState>().popup_gen == generation {
                            hide(cx);
                        }
                    });
                    return;
                }
                Ok(Some(false)) => cx.background_executor().timer(HOVER_RECHECK).await,
                _ => return,
            }
        }
    })
    .detach();
}

/// Whether to show an extension's pop-up, given which page the main window shows and whether it is the window
/// you're using: not when you're already looking at that extension's page.
pub fn should_show(slot: samo_extensions::ExtSlot, looking: Option<(NavSection, bool)>) -> bool {
    !matches!(looking, Some((NavSection::Extension(on_screen), true)) if on_screen == slot)
}

/// Hides the pop-up showing, if any.
/// Hides the pop-up showing, if any. Not from the pop-up's own event handlers: gpui can't reach a window while it
/// is handling one of its events, so those use `close_from_inside`.
pub fn hide(cx: &mut App) {
    if let Some(handle) = cx.global_mut::<AppState>().popup.take() {
        let _ = handle.update(cx, |_, window, _| window.remove_window());
    }
}

/// Closes the pop-up from one of its own event handlers (the close button, a click on the card).
fn close_from_inside(window: &mut Window, cx: &mut App) {
    let state = cx.global_mut::<AppState>();
    state.popup = None;
    // Any hide timer still running belongs to a pop-up that is gone.
    state.popup_gen = state.popup_gen.wrapping_add(1);
    window.remove_window();
}

pub struct PopupView {
    popup: Popup,
    /// When it was shown, `08:44`.
    at: String,
    hovered: bool,
    leaving: bool,
    /// Bumped when a newer pop-up replaces this one, so it arrives again.
    serial: u64,
}

/// The kind's colour: Ink when something waits for you, Cobalt when something finished.
fn kind_color(kind: PopupKind) -> Hsla {
    let pal = p();
    match kind {
        PopupKind::NeedsYou => pal.text,
        PopupKind::Finished | PopupKind::Info => pal.accent,
    }
}

/// The kind in Samo's ring: a dial with a check (finished) or a plug (a note), or an Ink ring with `!`.
fn kind_mark(kind: PopupKind) -> gpui::AnyElement {
    let pal = p();
    match kind {
        PopupKind::Finished => dial_chip("check", 38.0).into_any_element(),
        PopupKind::Info => dial_chip("plug", 38.0).into_any_element(),
        PopupKind::NeedsYou => div()
            .size(px(38.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(pal.shell)
            .border_2()
            .border_color(pal.text)
            .font(mono(FontWeight::BOLD))
            .text_size(px(T_H2))
            .text_color(pal.text)
            .child("!")
            .into_any_element(),
    }
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
        let kind = self.popup.kind;
        let edge = kind_color(kind);

        let mono_line = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(S2))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(mono_label(&header(&self.popup.source, kind), pal.text_2)),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .font(mono(FontWeight::NORMAL))
                    .text_size(px(T_LABEL))
                    .text_color(pal.text_2)
                    .child(self.at.clone()),
            )
            .when(self.hovered, |row| {
                row.child(
                    div()
                        .id("popup-close")
                        .size(px(18.0))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(R_CHIP))
                        .hover(|s| s.bg(pal.hover))
                        .cursor_pointer()
                        .child(icon("close", 11.0, pal.text_2))
                        .on_mouse_down(MouseButton::Left, |_, window, cx| {
                            cx.stop_propagation();
                            close_from_inside(window, cx);
                        }),
                )
            });

        let card = div()
            .id("popup-card")
            .size_full()
            .relative()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(S3))
            .pl(px(16.0))
            .pr(px(12.0))
            .rounded(px(R_SHELL))
            .bg(pal.frame)
            .border_1()
            .border_color(pal.hairline_strong)
            .overflow_hidden()
            .text_color(pal.text)
            .cursor_pointer()
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                this.hovered = *hovered;
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    let target = this.popup.target;
                    close_from_inside(window, cx);
                    open_main(cx, target);
                }),
            )
            // The kind's colour: a short rounded bar inside the left edge.
            .child(
                div()
                    .absolute()
                    .left(px(6.0))
                    .top(px(18.0))
                    .bottom(px(18.0))
                    .w(px(3.0))
                    .rounded_full()
                    .bg(edge),
            )
            .child(kind_mark(kind))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .child(mono_line)
                    .child(
                        line(self.popup.title.clone(), FontWeight::MEDIUM, 15.0, pal.text).w_full(),
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
            );

        // gpui updates hover only on moves inside the window; when the pointer leaves it, say so here.
        let this = cx.entity();
        let watch_exit = canvas(
            |_, _, _| {},
            move |_, _, window, _| {
                let this = this.clone();
                window.on_mouse_event(move |_: &MouseExitEvent, _, _, cx| {
                    this.update(cx, |view, cx| {
                        if view.hovered {
                            view.hovered = false;
                            cx.notify();
                        }
                    });
                });
            },
        )
        .absolute()
        .size_full();

        let leaving = self.leaving;
        div().size_full().child(watch_exit).child(
            div()
                .size_full()
                .relative()
                .child(card)
                // Arrives: a short drop and fade. Leaves: a fade.
                .with_animation(
                    gpui::ElementId::Name(
                        format!(
                            "popup-{}-{}",
                            self.serial,
                            if leaving { "leave" } else { "arrive" }
                        )
                        .into(),
                    ),
                    Animation::new(if leaving { LEAVE } else { ARRIVE })
                        .with_easing(ease_out_quint()),
                    move |el, t| {
                        if leaving {
                            el.opacity(1.0 - t)
                        } else {
                            el.opacity(t).top(px(-8.0 * (1.0 - t)))
                        }
                    },
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
    fn popup_skipped_when_page_visible() {
        use samo_extensions::ExtSlot;
        let mine = ExtSlot(1);
        assert!(!should_show(
            mine,
            Some((NavSection::Extension(mine), true))
        ));
        // Another page, another extension's page, a window in the background, or no window: show it.
        assert!(should_show(mine, Some((NavSection::Overview, true))));
        assert!(should_show(
            mine,
            Some((NavSection::Extension(ExtSlot(2)), true))
        ));
        assert!(should_show(
            mine,
            Some((NavSection::Extension(mine), false))
        ));
        assert!(should_show(mine, None));
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

    #[test]
    fn header_says_who_and_what() {
        assert_eq!(
            header("Claude Sessions", PopupKind::Finished),
            "CLAUDE SESSIONS · FINISHED"
        );
        assert_eq!(
            header("Claude Sessions", PopupKind::NeedsYou),
            "CLAUDE SESSIONS · NEEDS YOU"
        );
        assert_eq!(header("", PopupKind::Info), "NOTE");
    }
}
