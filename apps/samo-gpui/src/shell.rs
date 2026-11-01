//! Samo's frame: a sidebar (an icon rail when the window is narrow), and a stage set into the frame with the
//! measuring grid along its top. Pinned at the top of the stage is the **readout** — Samo's instrument
//! cluster, a dark window with phosphor figures — and under it each page: its dial, its title, the line
//! that says what just happened, and its body. The Trash dialog lives here too.

use crate::art::{
    Mood, Who, cell, dashed_rule, dial_chip, dither_block, dot_number, dot_width, led_meter,
    logo_mark, measure_grid,
};
use crate::charts::LineChart;
use crate::components::*;
use crate::overview::cpu_series;
use crate::theme::*;
use crate::{Frame, NavSection, NoticeKind, PendingTrashConfirm, SamoWorkspaceApp, locations};
use gpui::{
    Animation, AnimationExt, AnyElement, ClickEvent, Context, Corner, Div, ElementId, FontWeight,
    IntoElement, ParentElement, PathPromptOptions, SharedString, Stateful, Styled, Window,
    anchored, deferred, div, ease_out_quint, prelude::*, px,
};
use samo_primitives::{MemoryPressure, Reading, ScanSnapshot};
use std::time::Duration;
use std::time::Instant;

/// What a page puts in its header: its icon on a dial, one `h1`, one line on what it holds, its actions.
pub struct Head {
    pub icon: &'static str,
    pub title: String,
    pub sub: String,
    /// Show the scanned folder as a path above the title.
    pub crumbs: bool,
    pub actions: Vec<AnyElement>,
}

pub struct Page {
    pub head: Head,
    pub body: Div,
    /// The page fills the stage and scrolls inside itself (a long table), instead of scrolling as a whole.
    pub fill: bool,
}

/// Breadcrumb segments for a path: `(label, absolute path)`, home shown as `~`.
pub fn path_segments(root: &str) -> Vec<(String, String)> {
    let home = home_dir();
    let under_home = !home.is_empty() && (root == home || root.starts_with(&format!("{home}/")));
    let (mut acc, mut out, rest) = if under_home {
        (
            home.clone(),
            vec![("~".to_string(), home.clone())],
            root[home.len()..].to_string(),
        )
    } else {
        (
            String::new(),
            vec![("Macintosh HD".to_string(), "/".to_string())],
            root.to_string(),
        )
    };
    for part in rest.split('/').filter(|p| !p.is_empty()) {
        acc = format!("{acc}/{part}");
        out.push((part.to_string(), acc.clone()));
    }
    out
}

/// The standard line under a disk page's title.
pub fn scan_sub(frame: &Frame, detail: impl FnOnce(&ScanSnapshot) -> String) -> String {
    match (frame.current_disk(), frame.is_scanning, &frame.scan_error) {
        (Some(disk), _, _) => detail(disk),
        (None, true, _) => format!("Samo is measuring {}.", display_path(&frame.scan_root)),
        (None, false, Some(_)) => format!("Samo couldn’t read {}.", display_path(&frame.scan_root)),
        (None, false, None) => display_path(&frame.scan_root),
    }
}

/// Samo's own state, as its probe shows it.
pub fn samo_mood(frame: &Frame) -> Mood {
    if frame.is_scanning {
        Mood::Working
    } else if frame.scan_error.is_some() {
        Mood::Blocked
    } else if !frame.telemetry.provider_healthy || !frame.telemetry_ready {
        Mood::Asleep
    } else {
        Mood::Idle
    }
}

/// The scanning row for a card whose data hasn't come yet, or why it can't.
pub fn pending(frame: &Frame) -> Div {
    match (&frame.scan_error, frame.is_scanning) {
        (Some(err), false) => error_line(&format!(
            "{err} Pick another folder in the sidebar, or try Rescan."
        )),
        _ => loading(&format!("Measuring {}…", display_path(&frame.scan_root))),
    }
}

/// One cell of the readout.
pub(crate) struct Readout {
    id: &'static str,
    label: &'static str,
    context: String,
    figure: Option<String>,
    unit: String,
    /// Signal lamp: something is wrong.
    wrong: bool,
    /// Dithered lamp: this figure is Samo's suggestion.
    suggestion: bool,
    visual: AnyElement,
    pub(crate) go: NavSection,
}

impl SamoWorkspaceApp {
    // ---------------------------------------------------------------- the sidebar

    pub(crate) fn render_sidebar(&self, frame: &Frame, cx: &mut Context<Self>) -> AnyElement {
        if frame.layout.rail {
            return self.render_rail(frame, cx).into_any_element();
        }
        let pal = p();
        let project_count = frame.view().map(|v| v.snap.projects.len());
        let process_count = frame.telemetry.total_processes;
        let sub = if frame.telemetry.system.hostname.is_empty() {
            "Mac instrument".to_string()
        } else {
            frame.telemetry.system.hostname.clone()
        };

        div()
            .w(px(SIDEBAR_W))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap(px(S3))
            .px(px(14.0))
            .pb(px(14.0))
            .child(titlebar_band())
            .child(
                div()
                    .px(px(6.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(S3))
                    .child(logo_mark(34.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .font(display())
                                    .text_size(px(T_LEAD))
                                    .line_height(px(20.0))
                                    .text_color(pal.text)
                                    .child("Samo"),
                            )
                            .child(line(sub, FontWeight::NORMAL, T_META, pal.text_2).w_full()),
                    ),
            )
            .child(
                div()
                    .id("rail-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .pt(px(S2))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(rail_label("Monitor", true))
                    .children(NavSection::ALL.into_iter().map(|section| {
                        let badge = match section {
                            NavSection::Projects => project_count.map(|n| n.to_string()),
                            NavSection::Processes if process_count > 0 => {
                                Some(format_count(process_count as u64))
                            }
                            _ => None,
                        };
                        self.nav_link(section, section.title().to_string(), badge, cx)
                    }))
                    .child(rail_label("Extensions", false))
                    .children(frame.ext.extensions.iter().filter(|e| e.on).map(|e| {
                        let badge = (e.badge > 0).then(|| format!("!{}", e.badge));
                        self.nav_link(NavSection::Extension(e.slot), e.name.clone(), badge, cx)
                    }))
                    .child(self.nav_link(
                        NavSection::Extensions,
                        "All extensions".to_string(),
                        updates_badge(frame),
                        cx,
                    ))
                    .child(rail_label("Samo", false))
                    .child(self.nav_link(NavSection::Settings, "Settings".to_string(), None, cx)),
            )
            .child(self.side_foot(frame))
            .into_any_element()
    }

    fn nav_link(
        &self,
        section: NavSection,
        label: String,
        badge: Option<String>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let pal = p();
        let current = self.nav == section;
        div()
            .id(SharedString::from(format!("nav-{}", section.key())))
            .h(px(34.0))
            .px(px(10.0))
            .flex()
            .flex_row()
            .flex_shrink_0()
            .items_center()
            .gap(px(10.0))
            .rounded(px(R_CONTROL))
            .border_1()
            .cursor_pointer()
            .when(current, |el| el.bg(pal.shell).border_color(pal.hairline))
            .when(!current, |el| {
                el.border_color(gpui::transparent_black())
                    .hover(|s| s.bg(pal.hover))
            })
            .on_click(cx.listener(move |this, _, _, cx| this.nav_clicked(section, cx)))
            .child(icon(
                section.icon(),
                17.0,
                if current { pal.accent_text } else { pal.text_2 },
            ))
            .child(
                div()
                    .flex_1()
                    .font(body(FontWeight::MEDIUM))
                    .text_size(px(T_BODY))
                    .text_color(pal.text)
                    .child(label),
            )
            .children(badge.map(|b| {
                div()
                    .font(mono(FontWeight::NORMAL))
                    .text_size(px(12.0))
                    .text_color(pal.text_2)
                    .child(b)
            }))
    }

    /// The narrow-window sidebar: the same places as icons, each with its name on hover.
    fn render_rail(&self, frame: &Frame, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = p();
        let rail_button =
            |id: SharedString, icon_name: &'static str, name: String, current: bool| {
                div()
                    .id(id)
                    .size(px(40.0))
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .rounded(px(R_CONTROL))
                    .border_1()
                    .cursor_pointer()
                    .when(current, |el| el.bg(pal.shell).border_color(pal.hairline))
                    .when(!current, |el| {
                        el.border_color(gpui::transparent_black())
                            .hover(|s| s.bg(pal.hover))
                    })
                    .tooltip(move |_, cx| tip(name.clone().into(), cx))
                    .child(icon(
                        icon_name,
                        18.0,
                        if current { pal.accent_text } else { pal.text_2 },
                    ))
            };

        div()
            .w(px(RAIL_W))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(S2))
            .pb(px(S3))
            .child(titlebar_band())
            .child(logo_mark(32.0))
            .child(
                div()
                    .id("rail-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .pt(px(S3))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(S1))
                    .children(
                        NavSection::ALL
                            .into_iter()
                            .map(|section| (section, section.title().to_string()))
                            .chain(
                                frame
                                    .ext
                                    .extensions
                                    .iter()
                                    .filter(|e| e.on)
                                    .map(|e| (NavSection::Extension(e.slot), e.name.clone())),
                            )
                            .chain([
                                (NavSection::Extensions, "All extensions".to_string()),
                                (NavSection::Settings, "Settings".to_string()),
                            ])
                            .map(|(section, name)| {
                                rail_button(
                                    SharedString::from(format!("rail-{}", section.key())),
                                    section.icon(),
                                    name,
                                    self.nav == section,
                                )
                                .on_click(
                                    cx.listener(move |this, _, _, cx| {
                                        this.nav_clicked(section, cx)
                                    }),
                                )
                            }),
                    ),
            )
            .child(
                div()
                    .id("rail-samo")
                    .tooltip({
                        let words = SharedString::from(samo_status(frame));
                        move |_, cx| tip(words.clone(), cx)
                    })
                    .child(cell(Who::Samo, samo_mood(frame), 1.5)),
            )
    }

    fn side_foot(&self, frame: &Frame) -> impl IntoElement {
        let pal = p();
        let volume = frame.volume();
        let used = volume.map(|d| ratio(d.volume_used_bytes, d.volume_total_bytes));
        let nearly_full = used.is_some_and(|r| r >= 0.95);
        let disk_mood = match (volume, nearly_full) {
            (None, _) => Mood::Asleep,
            (Some(_), true) => Mood::Blocked,
            (Some(_), false) => Mood::Idle,
        };
        let volume_name = volume
            .map(|d| {
                if d.volume_name.is_empty() {
                    "Macintosh HD".to_string()
                } else {
                    d.volume_name.clone()
                }
            })
            .unwrap_or_else(|| "Macintosh HD".to_string());
        let volume_words = volume
            .map(|d| {
                format!(
                    "{} free of {}",
                    format_bytes(d.volume_free_bytes),
                    format_bytes(d.volume_total_bytes)
                )
            })
            .unwrap_or_else(|| "Not measured yet".to_string());
        let user = std::env::var("USER").unwrap_or_else(|_| "you".to_string());
        let uptime = frame.telemetry.system.uptime_secs;

        let row = |who: Who, mood: Mood, title: String, words: String| {
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(10.0))
                .child(cell(who, mood, 1.5))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .child(line(title, FontWeight::SEMIBOLD, 13.5, pal.text).w_full())
                        .child(line(words, FontWeight::NORMAL, 12.0, pal.text_2).w_full()),
                )
        };

        div()
            .flex_shrink_0()
            .pt(px(S3))
            .border_t_1()
            .border_color(pal.hairline)
            .flex()
            .flex_col()
            .gap(px(S2))
            // Samo, present: its probe shows what the scanner is really doing
            .child(
                div()
                    .py(px(S2))
                    .px(px(10.0))
                    .rounded(px(R_SHELL - 2.0))
                    .bg(pal.shell)
                    .border_1()
                    .border_color(pal.hairline)
                    .child(row(
                        Who::Samo,
                        samo_mood(frame),
                        "Samo".to_string(),
                        samo_status(frame),
                    )),
            )
            .child(
                div()
                    .px(px(10.0))
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(row(Who::Disk, disk_mood, volume_name, volume_words))
                    .child(led_meter(
                        used,
                        if nearly_full { pal.signal } else { pal.accent },
                        pal.dot,
                        5.0,
                    )),
            )
            .child(div().px(px(10.0)).child(row(
                Who::Person(person_tint(&user)),
                Mood::Idle,
                user,
                if uptime > 0 {
                    format!("Mac up {}", format_uptime(uptime))
                } else {
                    "This Mac".to_string()
                },
            )))
    }

    // ---------------------------------------------------------------- the stage

    pub(crate) fn render_stage(
        &self,
        page: Page,
        frame: &Frame,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let pal = p();
        let pad = frame.layout.pad;
        let fill = page.fill;
        let column = div()
            .w_full()
            .px(px(pad))
            .pt(px(S5))
            .flex()
            .flex_col()
            .gap(px(S5))
            .when(fill, |el| el.flex_1().min_h(px(0.0)).pb(px(pad.min(S5))))
            .when(!fill, |el| el.pb(px(48.0)))
            .child(self.page_head(page.head, frame, cx))
            .children(self.notice_line(frame, cx))
            .child(if fill {
                page.body.flex_1().min_h(px(0.0))
            } else {
                page.body
            })
            // A page arrives: a short fade, once, when it opens.
            .with_animation(
                ElementId::Name(format!("arrive-{}", self.nav.key()).into()),
                Animation::new(Duration::from_millis(180)).with_easing(ease_out_quint()),
                |el, t| el.opacity(t),
            );

        div()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .py(px(crate::layout::STAGE_INSET))
            .pr(px(crate::layout::STAGE_INSET))
            .child(
                div()
                    .size_full()
                    .relative()
                    .flex()
                    .flex_col()
                    .rounded(px(R_STAGE))
                    .bg(pal.stage)
                    .border_1()
                    .border_color(pal.hairline)
                    .overflow_hidden()
                    .child(measure_grid(320.0))
                    .child(
                        div()
                            .flex_shrink_0()
                            .px(px(pad))
                            .pt(px(S4))
                            .child(self.readout_strip(frame, cx)),
                    )
                    .child(if fill {
                        div()
                            .flex_1()
                            .min_h(px(0.0))
                            .flex()
                            .flex_col()
                            .child(column)
                            .into_any_element()
                    } else {
                        div()
                            .id(SharedString::from(format!("page-{}", self.nav.key())))
                            .flex_1()
                            .min_h(px(0.0))
                            .overflow_y_scroll()
                            .child(column)
                            .into_any_element()
                    }),
            )
    }

    /// Samo's instrument cluster: four live figures in a dark window, phosphor-lit, on every page.
    /// Always one row: on a narrow window the figures shrink and the side labels step away, so the readout
    /// never eats the height a short window needs for the page.
    fn readout_strip(&self, frame: &Frame, cx: &mut Context<Self>) -> impl IntoElement {
        let cols = 4.0;
        let pad_x = if frame.layout.roomy() { S4 } else { S3 };
        let show_context = frame.layout.content_w / cols >= 190.0;
        let items = readouts(frame);
        let dot_h = readout_dot_h(
            &items,
            frame.layout.content_w / cols - 2.0 * pad_x - 30.0,
            30.0,
        );

        glass_panel().child(
            div()
                .flex()
                .flex_row()
                .children(items.into_iter().enumerate().map(|(i, ro)| {
                    let go = ro.go;
                    readout_cell(ro, dot_h, pad_x, show_context, i > 0)
                        .on_click(cx.listener(move |this, _, _, cx| this.set_nav(go, cx)))
                })),
        )
    }

    fn page_head(&self, head: Head, frame: &Frame, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = p();
        let crumbs = head.crumbs.then(|| {
            let segments = path_segments(&frame.scan_root);
            let last = segments.len().saturating_sub(1);
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap(px(6.0))
                .children(segments.into_iter().enumerate().map(|(i, (label, path))| {
                    let is_last = i == last;
                    let text = mono_label(&label, if is_last { pal.text } else { pal.text_2 });
                    let crumb: AnyElement = if is_last {
                        text.into_any_element()
                    } else {
                        div()
                            .id(SharedString::from(format!("crumb-{i}")))
                            .cursor_pointer()
                            .rounded(px(4.0))
                            .hover(|s| s.bg(pal.hover))
                            .tooltip({
                                let t =
                                    SharedString::from(format!("Measure {}", display_path(&path)));
                                move |_, cx| tip(t.clone(), cx)
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.trigger_scan(path.clone(), cx)
                            }))
                            .child(text)
                            .into_any_element()
                    };
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(6.0))
                        .when(i > 0, |el| el.child(mono_label("/", pal.text_2)))
                        .child(crumb)
                }))
        });

        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(px(S4))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(14.0))
                    .child(dial_chip(head.icon, 44.0))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .gap(px(3.0))
                            .children(crumbs)
                            .child(h1(head.title))
                            .child(
                                div()
                                    .font(body(FontWeight::NORMAL))
                                    .text_size(px(T_BODY))
                                    .text_color(pal.text_2)
                                    .child(head.sub),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap(px(S2))
                    .children(head.actions),
            )
    }

    /// The last outcome (a trash, a quit, an error), under the title. A trash can be undone from here.
    fn notice_line(&self, frame: &Frame, cx: &mut Context<Self>) -> Option<Div> {
        let pal = p();
        // Progress already shows in the title's line, the cards and Samo's probe; this line is for outcomes.
        let notice = frame
            .notice
            .clone()
            .filter(|n| n.kind != NoticeKind::Progress)?;
        let (lead, color): (AnyElement, _) = match notice.kind {
            NoticeKind::Progress => return None,
            NoticeKind::Success => (
                icon("check", 15.0, pal.text_2).into_any_element(),
                pal.text_2,
            ),
            NoticeKind::Error => (
                cell(Who::Mascot, Mood::Blocked, 1.0).into_any_element(),
                pal.signal_text,
            ),
        };
        Some(
            div()
                .min_h(px(30.0))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(S2))
                .child(lead)
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .font(body(FontWeight::NORMAL))
                        .text_size(px(13.5))
                        .text_color(color)
                        .child(notice.text),
                )
                .when(frame.has_undo, |el| {
                    el.child(
                        button("notice-undo", "Undo", Some("undo"), Variant::Link, true)
                            .on_click(cx.listener(|this, _, _, cx| this.undo_last_trash(cx))),
                    )
                }),
        )
    }

    /// Which folder Samo measures, as a menu: the standard folders on this Mac, the ones you added, and
    /// "Choose folder…" (the system's folder picker). It replaces a fixed list in the sidebar, since no two
    /// Macs have the same folders.
    fn location_picker(&self, frame: &Frame, cx: &mut Context<Self>) -> AnyElement {
        let pal = p();
        let name = locations::name_for(&self.locations, &frame.scan_root);
        let current_icon = self
            .locations
            .iter()
            .find(|l| l.path == frame.scan_root)
            .map(|l| l.icon)
            .unwrap_or("folder");
        let show_name = frame.layout.roomy();

        let trigger = div()
            .id("loc-trigger")
            .h(px(CONTROL_H))
            .px(px(S3))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(S2))
            .rounded(px(R_CONTROL))
            .bg(pal.card)
            .border_1()
            .border_color(pal.hairline_strong)
            .cursor_pointer()
            .hover(|s| s.bg(pal.card_hover))
            .tooltip(|_, cx| tip("Choose what to measure".into(), cx))
            .on_click(cx.listener(|this, _, _, cx| {
                // A click on the trigger while the menu is open already closed it (mouse down outside).
                let just_closed = this
                    .location_menu_closed_at
                    .is_some_and(|t| t.elapsed().as_millis() < 250);
                this.location_menu = !this.location_menu && !just_closed;
                cx.notify();
            }))
            .child(icon(current_icon, 15.0, pal.accent_text))
            .when(show_name, |el| {
                el.child(
                    div()
                        .max_w(px(180.0))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .font(body(FontWeight::MEDIUM))
                        .text_size(px(T_BODY))
                        .text_color(pal.text)
                        .child(name),
                )
            })
            .child(icon("chevron-down", 12.0, pal.text_2));

        // The menu hangs from a zero-width marker at the trigger's bottom-right corner, so its right edge
        // lines up with the trigger's; it is drawn last (deferred) so it sits above the page.
        div()
            .relative()
            .child(trigger)
            .when(self.location_menu, |el| {
                el.child(
                    div().absolute().right_0().top(px(CONTROL_H + 6.0)).child(
                        deferred(
                            anchored()
                                .anchor(Corner::TopRight)
                                .snap_to_window_with_margin(px(8.0))
                                .child(self.location_menu_list(frame, cx)),
                        )
                        .with_priority(1),
                    ),
                )
            })
            .into_any_element()
    }

    fn location_menu_list(&self, frame: &Frame, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = p();
        let row = |id: SharedString| {
            div()
                .id(id)
                .h(px(40.0))
                .px(px(10.0))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(10.0))
                .rounded(px(R_CONTROL))
                .cursor_pointer()
                .hover(|s| s.bg(pal.hover))
        };

        div()
            .id("loc-menu")
            .w(px(300.0))
            .p(px(6.0))
            .flex()
            .flex_col()
            .rounded(px(R_CARD))
            .bg(pal.card)
            .border_1()
            .border_color(pal.hairline_strong)
            .shadow(shadow_pop())
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                if this.location_menu {
                    this.location_menu = false;
                    this.location_menu_closed_at = Some(Instant::now());
                    cx.notify();
                }
            }))
            .child(
                div()
                    .px(px(10.0))
                    .pt(px(6.0))
                    .pb(px(4.0))
                    .child(mono_label("Measure", pal.text_2)),
            )
            .children(self.locations.iter().enumerate().map(|(i, loc)| {
                let current = loc.path == frame.scan_root;
                let go = loc.path.clone();
                let forget = loc.path.clone();
                row(SharedString::from(format!("loc-{i}")))
                    .group("loc-row")
                    .on_click(cx.listener(move |this, _, _, cx| this.trigger_scan(go.clone(), cx)))
                    .child(icon(
                        loc.icon,
                        16.0,
                        if current { pal.accent_text } else { pal.text_2 },
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .child(
                                line(
                                    loc.name.clone(),
                                    if current {
                                        FontWeight::SEMIBOLD
                                    } else {
                                        FontWeight::MEDIUM
                                    },
                                    13.5,
                                    pal.text,
                                )
                                .w_full(),
                            )
                            .child(mono_line(display_path(&loc.path), 11.5, pal.text_2).w_full()),
                    )
                    .when(current, |el| el.child(icon("check", 14.0, pal.accent_text)))
                    .when(loc.custom, |el| {
                        el.child(
                            div()
                                .opacity(0.0)
                                .group_hover("loc-row", |s| s.opacity(1.0))
                                .child(
                                    icon_button(
                                        SharedString::from(format!("loc-forget-{i}")),
                                        "close",
                                        "Remove from this list",
                                        Variant::Link,
                                        true,
                                    )
                                    .on_click(cx.listener(
                                        move |this, _, _, cx| {
                                            cx.stop_propagation();
                                            let _ = locations::remove(&mut this.locations, &forget);
                                            cx.notify();
                                        },
                                    )),
                                ),
                        )
                    })
            }))
            .child(div().h(px(1.0)).mx(px(10.0)).my(px(4.0)).bg(pal.hairline))
            .child(
                row("loc-choose".into())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.location_menu = false;
                        let picked = cx.prompt_for_paths(PathPromptOptions {
                            files: false,
                            directories: true,
                            multiple: false,
                            prompt: Some("Measure".into()),
                        });
                        cx.spawn(async move |this, cx| {
                            let Ok(Ok(Some(paths))) = picked.await else {
                                return;
                            };
                            let Some(path) = paths.into_iter().next() else {
                                return;
                            };
                            let path = path.to_string_lossy().into_owned();
                            let _ = this.update(cx, |this, cx| {
                                let _ = locations::add(&mut this.locations, &path);
                                this.trigger_scan(path, cx);
                            });
                        })
                        .detach();
                        cx.notify();
                    }))
                    .child(icon("plus", 16.0, pal.text_2))
                    .child(
                        div()
                            .font(body(FontWeight::MEDIUM))
                            .text_size(px(13.5))
                            .text_color(pal.text)
                            .child("Choose folder…"),
                    ),
            )
    }

    /// Actions shared by the disk pages: reveal the folder, and rescan it (the view's one primary action).
    pub(crate) fn disk_actions(&self, frame: &Frame, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let finder_root = frame.scan_root.clone();
        vec![
            self.location_picker(frame, cx),
            icon_button(
                "head-reveal",
                "reveal",
                "Reveal in Finder",
                Variant::Default,
                false,
            )
            .on_click(
                cx.listener(move |this, _, _, cx| this.reveal_in_finder(finder_root.clone(), cx)),
            )
            .into_any_element(),
            if frame.is_scanning {
                busy_button("Measuring").into_any_element()
            } else {
                button(
                    "head-rescan",
                    "Rescan",
                    Some("refresh"),
                    Variant::Primary,
                    false,
                )
                .tooltip(|_, cx| tip("Measure this folder again (⌘R)".into(), cx))
                .on_click(cx.listener(|this, _, _, cx| this.rescan(cx)))
                .into_any_element()
            },
        ]
    }

    // ---------------------------------------------------------------- the Trash dialog

    pub(crate) fn render_confirm_dialog(
        &self,
        conf: PendingTrashConfirm,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let pal = p();
        let user = std::env::var("USER").unwrap_or_else(|_| "you".to_string());
        div()
            .id("confirm-backdrop")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .occlude()
            .bg(pal.backdrop)
            .flex()
            .items_center()
            .justify_center()
            .p(px(S5))
            .on_click(cx.listener(|this, _, _, cx| {
                this.pending_confirm = None;
                cx.notify();
            }))
            .child(
                div()
                    .id("confirm-dialog")
                    .w_full()
                    .max_w(px(460.0))
                    .p(px(S5))
                    .flex()
                    .flex_col()
                    .gap(px(S4))
                    .rounded(px(R_DIALOG))
                    .bg(pal.card)
                    .border_1()
                    .border_color(pal.hairline_strong)
                    .shadow(shadow_pop())
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_start()
                            .gap(px(14.0))
                            .child(cell(Who::Person(person_tint(&user)), Mood::Idle, 2.0))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .flex()
                                    .flex_col()
                                    .gap(px(6.0))
                                    .child(h2(format!("Move “{}” to the Trash?", conf.label)))
                                    .child(
                                        div()
                                            .font(body(FontWeight::NORMAL))
                                            .text_size(px(T_BODY))
                                            .line_height(px(20.0))
                                            .text_color(pal.text_2)
                                            .child(format!(
                                                "Samo suggested it; the decision is yours. It frees about {}, and you can undo it here or restore it from the Trash in Finder.",
                                                format_bytes(conf.bytes)
                                            )),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .p(px(S3))
                            .rounded(px(R_CONTROL + 2.0))
                            .bg(pal.shell)
                            .border_1()
                            .border_color(pal.hairline)
                            .flex()
                            .flex_col()
                            .gap(px(S2))
                            .child(mono_line(display_path(&conf.target_path), T_META, pal.text).w_full())
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap(px(S2))
                                    .child(icon("shield", 13.0, pal.text_2))
                                    .child(
                                        div()
                                            .font(body(FontWeight::NORMAL))
                                            .text_size(px(T_META))
                                            .text_color(pal.text_2)
                                            .child("Samo checks it is still the item it measured before moving it."),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .justify_end()
                            .gap(px(S2))
                            .child(button("confirm-cancel", "Cancel", None, Variant::Default, false).on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.pending_confirm = None;
                                    cx.notify();
                                }),
                            ))
                            .child(
                                button("confirm-trash", "Move to Trash", Some("trash"), Variant::Primary, false)
                                    .on_click(cx.listener(|this, _, _, cx| this.execute_confirmed_trash(cx))),
                            ),
                    ),
            )
    }
}

/// The live figures the readout shows, in order: CPU, memory, disk free, what Samo suggests reclaiming.
pub(crate) fn readouts(frame: &Frame) -> Vec<Readout> {
    let pal = p();
    let t = &frame.telemetry;
    let cpu = match t.latest.cpu_overall_pct {
        Reading::Available { value, .. } => Some(value),
        _ => None,
    };
    let memory = match &t.latest.memory {
        Reading::Available { value, .. } => Some(value.clone()),
        _ => None,
    };
    let mem_critical = memory
        .as_ref()
        .is_some_and(|m| m.pressure == MemoryPressure::Critical);
    let volume = frame.volume();
    let used = volume.map(|d| ratio(d.volume_used_bytes, d.volume_total_bytes));
    let nearly_full = used.is_some_and(|r| r >= 0.95);
    let reclaim = frame.reclaimable_bytes();

    let meter = |r: Option<f32>, wrong: bool| {
        led_meter(
            r,
            if wrong { pal.signal } else { pal.phosphor },
            pal.phosphor_off,
            4.0,
        )
        .into_any_element()
    };
    let (mem_fig, mem_unit) = memory.as_ref().map(|m| bytes_parts(m.used_bytes)).unzip();
    let (disk_fig, disk_unit) = volume.map(|d| bytes_parts(d.volume_free_bytes)).unzip();
    let (rec_fig, rec_unit) = reclaim.map(bytes_parts).unzip();

    vec![
        Readout {
            id: "ro-cpu",
            label: "CPU",
            context: if t.system.logical_cores > 0 {
                format!("{} cores", t.system.logical_cores)
            } else {
                String::new()
            },
            figure: cpu.map(|v| format!("{v:.1}")),
            unit: "%".to_string(),
            wrong: false,
            suggestion: false,
            visual: div()
                .w_full()
                .h(px(14.0))
                .child(LineChart::percent(&cpu_series(frame), 60, pal.phosphor, false).render())
                .into_any_element(),
            go: NavSection::Performance,
        },
        Readout {
            id: "ro-mem",
            label: "Memory",
            context: memory
                .as_ref()
                .map(|m| format!("of {}", format_bytes(m.total_bytes)))
                .unwrap_or_default(),
            figure: mem_fig,
            unit: mem_unit.unwrap_or("GB").to_string(),
            wrong: mem_critical,
            suggestion: false,
            visual: meter(
                memory.as_ref().map(|m| ratio(m.used_bytes, m.total_bytes)),
                mem_critical,
            ),
            go: NavSection::Performance,
        },
        Readout {
            id: "ro-disk",
            label: "Disk free",
            context: volume
                .map(|d| format!("of {}", format_bytes(d.volume_total_bytes)))
                .unwrap_or_default(),
            figure: disk_fig,
            unit: disk_unit.unwrap_or("GB").to_string(),
            wrong: nearly_full,
            suggestion: false,
            visual: meter(used, nearly_full),
            go: NavSection::Storage,
        },
        Readout {
            id: "ro-reclaim",
            label: "Reclaimable",
            context: frame
                .view()
                .map(|v| {
                    let n = v.projects_with_artifacts;
                    format!("{n} project{}", if n == 1 { "" } else { "s" })
                })
                .unwrap_or_default(),
            figure: rec_fig,
            unit: rec_unit.unwrap_or("GB").to_string(),
            wrong: false,
            suggestion: true,
            visual: meter(
                frame
                    .current_disk()
                    .map(|d| ratio(reclaim.unwrap_or(0), d.scanned_bytes)),
                false,
            ),
            go: NavSection::Projects,
        },
    ]
}

/// One figure height for a set of readouts: as big as the narrowest cell allows, up to `max`.
pub(crate) fn readout_dot_h(items: &[Readout], cell_w: f32, max: f32) -> f32 {
    let widest = items
        .iter()
        .map(|r| dot_width(r.figure.as_deref().unwrap_or("--"), 1.0))
        .fold(0.0_f32, f32::max)
        .max(0.01);
    (cell_w / widest).clamp(12.0, max)
}

/// The readout's dark window.
pub(crate) fn glass_panel() -> Div {
    let pal = p();
    div()
        .rounded(px(R_SHELL))
        .bg(pal.glass)
        .border_1()
        .border_color(pal.glass_line)
        .shadow(shadow_card())
        .flex()
        .flex_col()
        .overflow_hidden()
}

/// One readout cell: lamp + name (+ context), the figure in phosphor dots, and a meter or trace.
/// The caller decides what a click does.
pub(crate) fn readout_cell(
    ro: Readout,
    dot_h: f32,
    pad_x: f32,
    show_context: bool,
    divider: bool,
) -> Stateful<Div> {
    let pal = p();
    let lamp: AnyElement = if ro.suggestion {
        dither_block(6.0, 6.0, pal.phosphor).into_any_element()
    } else {
        div()
            .size(px(6.0))
            .bg(if ro.wrong { pal.signal } else { pal.phosphor })
            .into_any_element()
    };
    let figure_color = if ro.wrong { pal.signal } else { pal.phosphor };
    let mut hover_bg = pal.glass_text;
    hover_bg.a = 0.04;
    div()
        .id(ro.id)
        .relative()
        .flex_1()
        .min_w(px(0.0))
        .px(px(pad_x))
        .py(px(S3))
        .flex()
        .flex_col()
        .gap(px(S2))
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg))
        .when(divider, |el| el.child(dashed_rule(pal.glass_line)))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(7.0))
                .child(lamp)
                .child(mono_label(ro.label, pal.glass_text_2))
                .when(show_context, |el| {
                    el.child(clip_text_right(ro.context, pal.glass_text_2))
                }),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_end()
                .gap(px(6.0))
                .child(dot_number(
                    ro.figure.as_deref().unwrap_or("--"),
                    dot_h,
                    if ro.figure.is_some() {
                        figure_color
                    } else {
                        pal.glass_text_2
                    },
                    pal.phosphor_off,
                ))
                .child(
                    div()
                        .font(mono(FontWeight::MEDIUM))
                        .text_size(px(12.0))
                        .text_color(pal.glass_text_2)
                        .child(ro.unit),
                ),
        )
        .child(div().h(px(14.0)).flex().items_end().child(ro.visual))
}

/// What Samo is doing, in words.
pub(crate) fn samo_status(frame: &Frame) -> String {
    if frame.is_scanning {
        format!("Measuring {}", display_path(&frame.scan_root))
    } else if frame.scan_error.is_some() {
        format!("Can’t read {}", display_path(&frame.scan_root))
    } else if !frame.telemetry_ready {
        "Starting its sensors".to_string()
    } else if !frame.telemetry.provider_healthy {
        "Sensors stopped".to_string()
    } else {
        format!(
            "Watching {} processes",
            format_count(frame.telemetry.total_processes as u64)
        )
    }
}

/// The title bar's band: the traffic lights live here, and a double click zooms as it does natively.
fn titlebar_band() -> Stateful<Div> {
    div()
        .id("titlebar-band")
        .h(px(36.0))
        .w_full()
        .flex_shrink_0()
        .on_click(|event: &ClickEvent, window: &mut Window, _| {
            if event.click_count() == 2 {
                window.titlebar_double_click();
            }
        })
}

/// The store's update count for the sidebar, from the cached catalog: it never goes online.
fn updates_badge(frame: &Frame) -> Option<String> {
    let n = crate::store_view::update_count(
        frame.store.catalog.as_deref(),
        &frame.ext,
        crate::store_view::SAMO_VERSION,
    );
    match n {
        0 => None,
        1 => Some("1 update".into()),
        n => Some(format!("{n} updates")),
    }
}

fn rail_label(text: &str, first: bool) -> Div {
    div()
        .px(px(10.0))
        .pt(px(if first { 0.0 } else { S4 }))
        .pb(px(6.0))
        .child(mono_label(text, p().text_2))
}

/// A short context string pushed to the right of a readout's label.
fn clip_text_right(text: String, color: gpui::Hsla) -> Div {
    div().flex_1().min_w(px(0.0)).flex().justify_end().child(
        div()
            .font(mono(FontWeight::NORMAL))
            .text_size(px(T_LABEL))
            .text_color(color)
            .whitespace_nowrap()
            .overflow_hidden()
            .child(text),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_outside_home_start_at_the_volume() {
        let segs = path_segments("/Applications");
        assert_eq!(
            segs,
            vec![
                ("Macintosh HD".to_string(), "/".to_string()),
                ("Applications".to_string(), "/Applications".to_string()),
            ]
        );
        assert_eq!(path_segments("/").len(), 1);
    }

    #[test]
    fn segments_under_home_start_at_tilde() {
        let home = home_dir();
        if home.is_empty() {
            return;
        }
        let segs = path_segments(&format!("{home}/Developer/x"));
        let labels: Vec<&str> = segs.iter().map(|(l, _)| l.as_str()).collect();
        assert_eq!(labels, vec!["~", "Developer", "x"]);
        assert_eq!(segs[2].1, format!("{home}/Developer/x"));
    }
}
