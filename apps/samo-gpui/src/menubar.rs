//! Samo in the macOS menu bar.
//!
//! The item is Samo's probe (a template image, so macOS tints it for light or dark menu bars) with the CPU load
//! beside it. Its eyes are the scanner's real state: two dots when idle, a sweeping bar while measuring, an X
//! when a scan failed. A click opens the **quick panel** — Samo's readout and a few facts — without opening the
//! full window; clicking anywhere else closes it. The status item itself comes from `tray-icon`, which wraps
//! `NSStatusItem` safely (Samo's own code stays free of `unsafe`).

use crate::art::{Mood, Who, cell, logo_mark, probe_template, scan_step};
use crate::components::*;
use crate::layout::Layout;
use crate::shell::{glass_panel, readout_cell, readout_dot_h, readouts, samo_mood, samo_status};
use crate::theme::*;
use crate::{AppState, Frame, NavSection, SamoWorkspaceApp, open_main, snapshot};
use futures::StreamExt;
use gpui::{
    App, Bounds, Context, FontWeight, IntoElement, ParentElement, Render, Styled, Subscription,
    Window, WindowAppearance, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions,
    div, point, prelude::*, px, size,
};
use samo_primitives::Reading;
use std::time::{Duration, Instant};
use tray_icon::{Icon, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub const PANEL_W: f32 = 340.0;
pub const PANEL_H: f32 = 446.0;
/// One quiet line under the readout.
const FACT_H: f32 = 34.0;
/// The quick panel shows a line for at most this many extensions.
const MAX_EXT_FACTS: usize = 3;

/// What the menu bar item currently shows, so it is only redrawn when something changed.
#[derive(PartialEq)]
struct Shown {
    mood: Mood,
    step: i32,
    title: String,
}

pub struct MenuBar {
    tray: TrayIcon,
    shown: Option<Shown>,
}

impl MenuBar {
    /// Puts Samo in the menu bar and starts listening for clicks. `None` if macOS refused the item.
    pub fn install(cx: &mut App) -> Option<Self> {
        let (tx, mut rx) = futures::channel::mpsc::unbounded::<TrayIconEvent>();
        TrayIconEvent::set_event_handler(Some(move |event| {
            let _ = tx.unbounded_send(event);
        }));
        let tray = TrayIconBuilder::new()
            .with_icon_templated(probe_icon(Mood::Asleep, 0)?)
            .with_tooltip("Samo")
            .with_menu_on_left_click(false)
            .build()
            .ok()?;

        cx.spawn(async move |cx| {
            while let Some(event) = rx.next().await {
                if let TrayIconEvent::Click {
                    rect,
                    button_state: MouseButtonState::Down,
                    ..
                } = event
                    && cx.update(|cx| toggle_panel(rect, cx)).is_err()
                {
                    break;
                }
            }
        })
        .detach();

        Some(Self { tray, shown: None })
    }

    /// Where the item is on screen, in pixels; `None` before macOS has placed it.
    pub fn tray_rect(&self) -> Option<crate::popup::TrayRect> {
        self.tray.rect().map(|r| crate::popup::TrayRect {
            x: r.position.x,
            y: r.position.y,
            w: f64::from(r.size.width),
            h: f64::from(r.size.height),
        })
    }

    /// Brings the item up to date. Cheap when nothing changed (most ticks).
    fn refresh(&mut self, frame: &Frame) {
        let mood = samo_mood(frame);
        let next = Shown {
            mood,
            step: if mood == Mood::Working {
                scan_step()
            } else {
                0
            },
            title: compose_title(cpu_pct(frame), frame.ext.attention()),
        };
        let (icon_changed, title_changed) = match &self.shown {
            Some(s) => (
                s.mood != next.mood || s.step != next.step,
                s.title != next.title,
            ),
            None => (true, true),
        };
        if icon_changed && let Some(icon) = probe_icon(next.mood, next.step) {
            let _ = self.tray.set_icon_templated(Some(icon));
        }
        if title_changed {
            self.tray.set_title(Some(&next.title));
        }
        self.shown = Some(next);
    }
}

/// Called on every wake from the sampler or a scan.
pub fn refresh(cx: &mut App) {
    let hub = cx.global::<AppState>().hub.clone();
    let Some(frame) = snapshot(&hub, Layout::for_window(PANEL_W)) else {
        return;
    };
    cx.update_global::<AppState, _>(|state, _| {
        if let Some(menubar) = state.menubar.as_mut() {
            menubar.refresh(&frame);
        }
    });
}

/// The probe drawn 2 px per grid pixel with a 2/4 px margin: 36 x 44 px, which macOS shows at exactly 22 pt.
fn probe_icon(mood: Mood, step: i32) -> Option<Icon> {
    let (rgba, w, h) = probe_template(mood, step, 2, (2, 4));
    Icon::from_rgba(rgba, w, h).ok()
}

fn cpu_pct(frame: &Frame) -> Option<f32> {
    match frame.telemetry.latest.cpu_overall_pct {
        Reading::Available { value, .. } => Some(value),
        _ => None,
    }
}

/// `" 7%"`, `"37%"`: padded with figure spaces so the item doesn't shuffle as the figure changes width. While
/// extensions have something waiting for you, their count follows: `"23% !1"`. The probe's eyes stay Samo's own
/// state (DESIGN.md rule 3); the count is text beside it.
pub(crate) fn compose_title(cpu: Option<f32>, attention: u32) -> String {
    let figure = cpu
        .map(|value| pad_figure(format!("{:.0}%", value.clamp(0.0, 100.0))))
        .unwrap_or_default();
    match (attention, figure.is_empty()) {
        (0, _) => figure,
        (n, true) => format!("!{n}"),
        (n, false) => format!("{figure} !{n}"),
    }
}

fn pad_figure(s: String) -> String {
    let width = 4;
    let n = s.chars().count();
    if n >= width {
        s
    } else {
        format!("{}{s}", "\u{2007}".repeat(width - n))
    }
}

/// The main screen's points-to-pixels factor; the item's rectangle arrives in pixels.
pub(crate) fn main_screen_scale() -> f64 {
    objc2::MainThreadMarker::new()
        .and_then(objc2_app_kit::NSScreen::mainScreen)
        .map(|screen| screen.backingScaleFactor())
        .filter(|s| *s > 0.0)
        .unwrap_or(2.0)
}

fn toggle_panel(rect: tray_icon::Rect, cx: &mut App) {
    let state = cx.global::<AppState>();
    if let Some(handle) = state.panel {
        state.hub.want_busiest(false);
        cx.global_mut::<AppState>().panel = None;
        let _ = handle.update(cx, |_, window, _| window.remove_window());
        return;
    }
    // The click that took focus away already closed it; don't reopen it at once.
    if state
        .panel_closed_at
        .is_some_and(|t| t.elapsed() < Duration::from_millis(350))
    {
        return;
    }

    let scale = main_screen_scale();
    let icon_left = rect.position.x / scale;
    let icon_w = f64::from(rect.size.width) / scale;
    let icon_bottom = (rect.position.y + f64::from(rect.size.height)) / scale;
    let display_w = cx
        .primary_display()
        .map(|d| f32::from(d.bounds().size.width))
        .unwrap_or(1440.0);
    let x = ((icon_left + icon_w / 2.0) as f32 - PANEL_W / 2.0)
        .clamp(8.0, (display_w - PANEL_W - 8.0).max(8.0));
    let y = icon_bottom as f32 + 6.0;

    let ext_lines = state
        .hub
        .ext
        .get()
        .map_or(0, |host| ext_facts_count(&host.snapshot()));
    let panel_h = PANEL_H + FACT_H * ext_lines as f32;
    let opened = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(x), px(y)),
                size: size(px(PANEL_W), px(panel_h)),
            })),
            titlebar: None,
            focus: true,
            show: true,
            kind: WindowKind::PopUp,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            window_background: WindowBackgroundAppearance::Transparent,
            ..Default::default()
        },
        |window, cx| cx.new(|cx| QuickPanel::new(window, cx)),
    );
    if let Ok(handle) = opened {
        let state = cx.global_mut::<AppState>();
        state.panel = Some(handle);
        state.hub.want_busiest(true);
    }
}

/// The quick panel: Samo's readout and a few facts, with a way into the full app.
pub struct QuickPanel {
    _activation: Subscription,
    _appearance: Subscription,
}

impl QuickPanel {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Clicking anywhere else closes it, as a menu-bar panel does.
        let activation = cx.observe_window_activation(window, |_, window, cx| {
            if !window.is_window_active() {
                cx.update_global::<AppState, _>(|state, _| {
                    state.panel = None;
                    state.panel_closed_at = Some(Instant::now());
                    state.hub.want_busiest(false);
                });
                window.remove_window();
            }
        });
        let appearance = cx.observe_window_appearance(window, |_, _, cx| cx.notify());
        Self {
            _activation: activation,
            _appearance: appearance,
        }
    }

    fn close(window: &mut Window, cx: &mut App) {
        cx.update_global::<AppState, _>(|state, _| {
            state.panel = None;
            state.panel_closed_at = Some(Instant::now());
            state.hub.want_busiest(false);
        });
        window.remove_window();
    }
}

impl Render for QuickPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        set_dark(pinned_dark().unwrap_or_else(|| {
            matches!(
                window.appearance(),
                WindowAppearance::Dark | WindowAppearance::VibrantDark
            )
        }));
        let pal = p();
        let hub = cx.global::<AppState>().hub.clone();
        let Some(frame) = snapshot(&hub, Layout::for_window(PANEL_W)) else {
            return div()
                .size_full()
                .bg(pal.frame)
                .child("Samo lost track of its state.");
        };

        let items = readouts(&frame);
        let dot_h = readout_dot_h(&items, PANEL_W / 2.0 - 2.0 * S3 - 40.0, 22.0);
        let mut cells = items.into_iter().map(|ro| {
            let go = ro.go;
            (ro, go)
        });
        let row = |pair: Vec<(crate::shell::Readout, NavSection)>, cx: &mut Context<Self>| {
            div()
                .flex()
                .flex_row()
                .children(pair.into_iter().enumerate().map(|(i, (ro, go))| {
                    readout_cell(ro, dot_h, S3, false, i > 0).on_click(cx.listener(
                        move |_, _, window, cx| {
                            open_main(cx, Some(go));
                            QuickPanel::close(window, cx);
                        },
                    ))
                }))
        };
        let top: Vec<_> = cells.by_ref().take(2).collect();
        let bottom: Vec<_> = cells.collect();

        div()
            .size_full()
            .flex()
            .flex_col()
            .rounded(px(R_SHELL + 2.0))
            .bg(pal.frame)
            .border_1()
            .border_color(pal.hairline_strong)
            .overflow_hidden()
            .text_color(pal.text)
            .font(body(FontWeight::NORMAL))
            // who and what
            .child(
                div()
                    .h(px(58.0))
                    .flex_shrink_0()
                    .px(px(14.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(10.0))
                    .child(logo_mark(28.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .font(display())
                                    .text_size(px(15.0))
                                    .line_height(px(18.0))
                                    .child("Samo"),
                            )
                            .child(
                                line(samo_status(&frame), FontWeight::NORMAL, 12.0, pal.text_2)
                                    .w_full(),
                            ),
                    )
                    .child(cell(Who::Samo, samo_mood(&frame), 1.5)),
            )
            // the readout, the same instrument as in the app
            .child(
                div().px(px(10.0)).child(
                    glass_panel()
                        .child(row(top, cx))
                        .child(row(bottom, cx).border_t_1().border_color(pal.glass_line)),
                ),
            )
            // a few facts
            .child(
                div()
                    .px(px(14.0))
                    .pt(px(6.0))
                    .flex()
                    .flex_col()
                    .children(facts(&frame))
                    .children(ext_facts(&frame).into_iter().map(|(fact, go)| {
                        fact.on_click(cx.listener(move |_, _, window, cx| {
                            open_main(cx, go);
                            QuickPanel::close(window, cx);
                        }))
                    })),
            )
            .child(div().flex_1())
            // into the app, or out
            .child(
                div()
                    .flex_shrink_0()
                    .px(px(10.0))
                    .py(px(10.0))
                    .border_t_1()
                    .border_color(pal.hairline)
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(S2))
                    .child(
                        button(
                            "qp-open",
                            "Open Samo",
                            Some("window"),
                            Variant::Primary,
                            true,
                        )
                        .flex_1()
                        .on_click(cx.listener(|_, _, window, cx| {
                            open_main(cx, None);
                            QuickPanel::close(window, cx);
                        })),
                    )
                    .child(
                        icon_button(
                            "qp-rescan",
                            "refresh",
                            "Measure the folder again",
                            Variant::Default,
                            true,
                        )
                        .on_click(cx.listener(|_, _, _, cx| {
                            let hub = cx.global::<AppState>().hub.clone();
                            let root = hub
                                .shared
                                .lock()
                                .map(|g| g.scan_root.clone())
                                .unwrap_or_default();
                            if !root.is_empty() && !hub.is_scanning() {
                                SamoWorkspaceApp::spawn_scan(&hub, root, true);
                            }
                            cx.notify();
                        })),
                    )
                    .child(
                        icon_button("qp-quit", "power", "Quit Samo", Variant::Link, true)
                            .on_click(|_, _, cx| crate::quit(cx)),
                    ),
            )
    }
}

/// Four quiet lines under the readout: the busiest process, memory, the network, the folder.
fn facts(frame: &Frame) -> Vec<gpui::Div> {
    let pal = p();
    let t = &frame.telemetry;
    let busiest = frame
        .busiest
        .as_ref()
        .map(|(name, cpu)| format!("{name} · {cpu:.0}%"))
        .unwrap_or_else(|| "—".to_string());
    let memory = match &t.latest.memory {
        Reading::Available { value, .. } => format!(
            "pressure {} · swap {}",
            crate::overview::pressure_status(value.pressure).1,
            format_bytes(value.swap_used_bytes)
        ),
        _ => "not sampled yet".to_string(),
    };
    let network = match &t.latest.network {
        Reading::Available { value, .. } => format!(
            "↓ {}  ↑ {}",
            format_rate(value.rx_bytes_per_sec),
            format_rate(value.tx_bytes_per_sec)
        ),
        _ => "not sampled yet".to_string(),
    };
    let folder = if frame.is_scanning {
        format!("measuring {}…", display_path(&frame.scan_root))
    } else {
        match frame.current_disk() {
            Some(d) => format!(
                "{} · {}",
                display_path(&d.root_path),
                d.completed_at_ms.map(format_ago).unwrap_or_default()
            ),
            None => display_path(&frame.scan_root),
        }
    };

    [
        ("pulse", "Busiest", busiest),
        ("chip", "Memory", memory),
        ("download", "Network", network),
        ("folder", "Folder", folder),
    ]
    .into_iter()
    .map(|(icon_name, label, value)| {
        div()
            .h(px(34.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(S2))
            .border_b_1()
            .border_color(pal.hairline)
            .child(icon(icon_name, 14.0, pal.text_2))
            .child(
                div()
                    .w(px(64.0))
                    .flex_shrink_0()
                    .text_size(px(13.0))
                    .text_color(pal.text)
                    .child(label),
            )
            .child(mono_line(value, 12.0, pal.text_2).flex_1())
    })
    .collect()
}

fn ext_facts_count(ext: &samo_extensions::ExtSnapshot) -> usize {
    ext.extensions
        .iter()
        .filter(|e| e.on)
        .count()
        .min(MAX_EXT_FACTS)
}

/// A line for each extension that is on: what it says is waiting, and how many. A click opens its page.
fn ext_facts(frame: &Frame) -> Vec<(gpui::Stateful<gpui::Div>, Option<NavSection>)> {
    let pal = p();
    frame
        .ext
        .extensions
        .iter()
        .filter(|e| e.on)
        .take(MAX_EXT_FACTS)
        .map(|e| {
            let text = if e.badge_summary.is_empty() {
                format!("{} · nothing waiting", e.name)
            } else {
                e.badge_summary.clone()
            };
            let fact = div()
                .id(("qp-ext", usize::from(e.slot.0)))
                .h(px(FACT_H))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(S2))
                .border_b_1()
                .border_color(pal.hairline)
                .cursor_pointer()
                .child(icon("plug", 14.0, pal.text_2))
                .child(line(text, FontWeight::NORMAL, 13.0, pal.text).flex_1())
                .when(e.badge > 0, |row| {
                    row.child(
                        div()
                            .flex_shrink_0()
                            .font(mono(FontWeight::BOLD))
                            .text_size(px(12.0))
                            .text_color(pal.text)
                            .child(format!("!{}", e.badge)),
                    )
                });
            (fact, None)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_appends_attention_count() {
        assert_eq!(compose_title(Some(23.4), 1), "\u{2007}23% !1");
        assert_eq!(compose_title(Some(100.0), 12), "100% !12");
        assert_eq!(compose_title(None, 2), "!2");
    }

    #[test]
    fn title_without_attention_is_unchanged() {
        assert_eq!(compose_title(Some(7.0), 0), "\u{2007}\u{2007}7%");
        assert_eq!(compose_title(Some(150.0), 0), "100%");
        assert_eq!(compose_title(None, 0), "");
    }

    #[test]
    fn titles_keep_one_width() {
        assert_eq!(pad_figure("7%".into()).chars().count(), 4);
        assert_eq!(pad_figure("37%".into()).chars().count(), 4);
        assert_eq!(pad_figure("100%".into()), "100%");
    }
}
