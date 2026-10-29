//! Overview: the disk as a ring (Samo's shape), what Samo suggests, and where the space went.

use crate::art::{ArcFill, RingPart, dither_block, dot_number, led_meter, ring_gauge};
use crate::components::*;
use crate::live::cadence_words;
use crate::shell::{Head, Page, pending, scan_sub};
use crate::theme::*;
use crate::{Frame, NavSection, SamoWorkspaceApp};
use gpui::{
    AnyElement, Context, Div, FontWeight, IntoElement, ParentElement, SharedString, Styled, div,
    prelude::*, px,
};
use samo_primitives::{MemoryPressure, ProjectSummary, Reading};

pub fn pressure_status(pressure: MemoryPressure) -> (bool, &'static str) {
    match pressure {
        MemoryPressure::Normal => (false, "normal"),
        MemoryPressure::Elevated => (false, "elevated"),
        MemoryPressure::Critical => (true, "critical"),
    }
}

pub fn cpu_series(frame: &Frame) -> Vec<Option<f32>> {
    frame
        .telemetry
        .history
        .iter()
        .map(|p| match p.cpu_overall_pct {
            Reading::Available { value, .. } => Some(value),
            _ => None,
        })
        .collect()
}

/// `node_modules ×12 · dist ×3 · target`, largest first.
pub fn artifact_summary(proj: &ProjectSummary) -> String {
    let mut groups: Vec<(&str, usize, u64)> = Vec::new();
    for a in &proj.artifacts {
        match groups.iter_mut().find(|g| g.0 == a.name) {
            Some(g) => {
                g.1 += 1;
                g.2 += a.bytes;
            }
            None => groups.push((a.name.as_str(), 1, a.bytes)),
        }
    }
    groups.sort_by_key(|g| std::cmp::Reverse(g.2));
    groups
        .into_iter()
        .map(|(name, n, _)| {
            if n > 1 {
                format!("{name} ×{n}")
            } else {
                name.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

/// A row in a card's list, running to the card's edges.
pub fn list_row(id: impl Into<gpui::ElementId>, group: &'static str) -> gpui::Stateful<Div> {
    let pal = p();
    div()
        .id(id)
        .group(group)
        .mx(px(-10.0))
        .px(px(10.0))
        .min_h(px(44.0))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(S3))
        .rounded(px(R_CONTROL))
        .hover(|s| s.bg(pal.hover))
}

/// Figures in a column: mono, right-aligned.
pub fn figure_cell(text: String, width: f32, strong: bool) -> Div {
    let pal = p();
    div()
        .w(px(width))
        .flex_shrink_0()
        .flex()
        .justify_end()
        .font(mono(if strong {
            FontWeight::MEDIUM
        } else {
            FontWeight::NORMAL
        }))
        .text_size(px(if strong { 13.5 } else { 12.0 }))
        .text_color(if strong { pal.text } else { pal.text_2 })
        .child(text)
}

/// A section of a page: its rule, then what it holds.
fn section(rule: Div, content: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(S4))
        .child(rule)
        .child(content)
}

/// Two blocks side by side when there is room, stacked when there isn't.
pub fn pair(
    wide: bool,
    first: impl IntoElement,
    second: impl IntoElement,
    second_w: Option<f32>,
) -> Div {
    div()
        .flex()
        .when(wide, |el| el.flex_row().items_start())
        .when(!wide, |el| el.flex_col())
        .gap(px(20.0))
        .child(div().flex_1().min_w(px(0.0)).w_full().child(first))
        .child(
            div()
                .when_some(second_w.filter(|_| wide), |el, w| {
                    el.w(px(w)).flex_shrink_0()
                })
                .when(second_w.is_none() || !wide, |el| {
                    el.flex_1().min_w(px(0.0)).w_full()
                })
                .child(second),
        )
}

impl SamoWorkspaceApp {
    pub(crate) fn render_overview(&self, frame: &Frame, cx: &mut Context<Self>) -> Page {
        let l = frame.layout;
        let sub = scan_sub(frame, |d| {
            let when = d
                .completed_at_ms
                .map(|ms| format!(" {}", format_ago(ms)))
                .unwrap_or_default();
            format!(
                "{} in {} files, measured{}.",
                format_bytes(d.scanned_bytes),
                format_count(d.scanned_files),
                when
            )
        });

        let live_about = if frame.telemetry_ready {
            format!(
                "{} · up {}",
                cadence_words(),
                format_uptime(frame.telemetry.system.uptime_secs)
            )
        } else {
            "starting sensors".to_string()
        };

        Page {
            head: Head {
                icon: "gauge",
                title: "Overview".to_string(),
                sub,
                crumbs: true,
                actions: self.disk_actions(frame, cx),
            },
            body: div()
                .flex()
                .flex_col()
                .gap(px(S6))
                .child(section(
                    section_rule("This Mac · live", Some(live_about)),
                    self.render_live(frame, cx),
                ))
                .child(section(
                    section_rule("Space", Some(display_path(&frame.scan_root))),
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(20.0))
                        .child(pair(
                            l.wide(),
                            disk_card(frame),
                            self.suggestions_card(frame, cx),
                            None,
                        ))
                        .child(pair(
                            l.wide(),
                            self.folders_card(frame, 8, cx),
                            file_types_card(frame),
                            Some(l.side_w()),
                        )),
                )),
            fill: false,
        }
    }

    /// Build artifacts Samo would clear. They are suggestions, so the title carries the dither swatch (the same
    /// one as the disk ring's legend); the card itself is an ordinary card.
    fn suggestions_card(&self, frame: &Frame, cx: &mut Context<Self>) -> Div {
        let pal = p();
        let head = div()
            .mb(px(S4))
            .min_h(px(26.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(S3))
            .child(dither_block(10.0, 10.0, pal.accent))
            .child(h2("Samo suggests"))
            .child(div().flex_1())
            .children(frame.reclaimable_bytes().map(format_bytes).map(count_chip))
            .child(
                button("ov-all-projects", "All projects", None, Variant::Link, true)
                    .on_click(cx.listener(|this, _, _, cx| this.set_nav(NavSection::Projects, cx))),
            );
        let body: Div = match frame.view() {
            None => pending(frame),
            Some(view) if view.projects_with_artifacts == 0 => empty(&format!(
                "Nothing to clear. No target, node_modules, .venv, .next or dist folders under {}.",
                display_path(&view.snap.root_path)
            )),
            Some(view) => {
                div().flex().flex_col().children(
                    view.projects_ranked
                        .iter()
                        .map(|&i| &view.snap.projects[i])
                        .filter(|p| p.artifact_bytes > 0)
                        .take(5)
                        .enumerate()
                        .map(|(i, proj)| {
                            list_row(SharedString::from(format!("ov-proj-{i}")), "ov-proj")
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.set_nav(NavSection::Projects, cx)
                                }))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .py(px(S2))
                                        .flex()
                                        .flex_col()
                                        .gap(px(2.0))
                                        .child(
                                            line(
                                                proj.name.clone(),
                                                FontWeight::SEMIBOLD,
                                                T_BODY,
                                                pal.text,
                                            )
                                            .w_full(),
                                        )
                                        .child(
                                            mono_line(artifact_summary(proj), 12.0, pal.text_2)
                                                .w_full(),
                                        ),
                                )
                                .when(frame.layout.roomy(), |el| {
                                    el.child(tag(proj.kind.clone(), TagKind::Neutral, false))
                                })
                                .child(figure_cell(format_bytes(proj.artifact_bytes), 84.0, true))
                        }),
                )
            }
        };
        card().child(head).child(body)
    }

    pub(crate) fn folders_card(&self, frame: &Frame, limit: usize, cx: &mut Context<Self>) -> Div {
        let pal = p();
        let head = card_head(
            "Largest folders",
            frame
                .current_disk()
                .map(|d| d.largest_folders.len().min(limit).to_string()),
            None,
        );
        let Some(disk) = frame.current_disk() else {
            return card().child(head).child(pending(frame));
        };
        if disk.largest_folders.is_empty() {
            return card()
                .child(head)
                .child(empty("No sub-folders here are large enough to list."));
        }
        let scanned = disk.scanned_bytes;
        let show_meter = frame.layout.content_w >= 900.0;

        card().child(head).child(
            div().flex().flex_col().children(
                disk.largest_folders
                    .iter()
                    .take(limit)
                    .enumerate()
                    .map(|(i, folder)| {
                        let open_path = folder.path.clone();
                        let reveal_path = folder.path.clone();
                        let trash_path = folder.path.clone();
                        let name = folder.name.clone();
                        let bytes = folder.bytes;
                        let identity = folder.identity;
                        let open_tip =
                            SharedString::from(format!("Open {}", display_path(&folder.path)));

                        list_row(SharedString::from(format!("folder-{i}")), "folder-row")
                            .cursor_pointer()
                            .tooltip(move |_, cx| tip(open_tip.clone(), cx))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.trigger_scan(open_path.clone(), cx)
                            }))
                            .child(icon("folder", 15.0, pal.text_2))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .py(px(6.0))
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.0))
                                    .child(
                                        line(
                                            folder.name.clone(),
                                            FontWeight::MEDIUM,
                                            T_BODY,
                                            pal.text,
                                        )
                                        .w_full(),
                                    )
                                    .child(
                                        mono_line(
                                            format!("{} files", format_count(folder.file_count)),
                                            12.0,
                                            pal.text_2,
                                        )
                                        .w_full(),
                                    ),
                            )
                            .when(show_meter, |el| {
                                el.child(div().w(px(88.0)).flex_shrink_0().child(led_meter(
                                    Some(ratio(folder.bytes, scanned)),
                                    pal.accent,
                                    pal.dot,
                                    5.0,
                                )))
                            })
                            .child(figure_cell(format_bytes(folder.bytes), 80.0, true))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .gap(px(2.0))
                                    .opacity(0.0)
                                    .group_hover("folder-row", |s| s.opacity(1.0))
                                    .child(
                                        icon_button(
                                            SharedString::from(format!("folder-reveal-{i}")),
                                            "reveal",
                                            "Reveal in Finder",
                                            Variant::Link,
                                            true,
                                        )
                                        .on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                cx.stop_propagation();
                                                this.reveal_in_finder(reveal_path.clone(), cx)
                                            }),
                                        ),
                                    )
                                    .child(
                                        icon_button(
                                            SharedString::from(format!("folder-trash-{i}")),
                                            "trash",
                                            "Move to Trash…",
                                            Variant::Link,
                                            true,
                                        )
                                        .on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                cx.stop_propagation();
                                                this.request_trash_confirmation(
                                                    name.clone(),
                                                    trash_path.clone(),
                                                    bytes,
                                                    identity,
                                                    cx,
                                                )
                                            }),
                                        ),
                                    ),
                            )
                    }),
            ),
        )
    }
}

/// The disk as a ring: this folder (Cobalt), what Samo suggests clearing (dithered), everything else (grey),
/// and the free space (the track). The free figure sits in the middle in dots.
fn disk_card(frame: &Frame) -> Div {
    let pal = p();
    let volume = frame.volume();
    let d = if frame.layout.wide() { 176.0 } else { 160.0 };

    let (parts, legend, free_fig, total_words) = match volume {
        None => (
            Vec::new(),
            Vec::new(),
            None,
            "Waiting for the first scan".to_string(),
        ),
        Some(v) => {
            let total = v.volume_total_bytes.max(1);
            let used = v.volume_used_bytes.min(total);
            let scanned = frame
                .current_disk()
                .map(|d| d.scanned_bytes.min(used))
                .unwrap_or(0);
            let reclaim = frame.reclaimable_bytes().unwrap_or(0).min(scanned);
            let share = |b: u64| ratio(b, total);
            let parts = vec![
                RingPart {
                    share: share(reclaim),
                    fill: ArcFill::Dither(pal.accent),
                },
                RingPart {
                    share: share(scanned - reclaim),
                    fill: ArcFill::Solid(pal.accent),
                },
                RingPart {
                    share: share(used - scanned),
                    fill: ArcFill::Solid(pal.text_2),
                },
            ];
            let legend: Vec<(AnyElement, &str, String)> = vec![
                (
                    div().size(px(10.0)).bg(pal.accent).into_any_element(),
                    "This folder",
                    format_bytes(scanned),
                ),
                (
                    dither_block(10.0, 10.0, pal.accent).into_any_element(),
                    "Samo suggests",
                    format_bytes(reclaim),
                ),
                (
                    div().size(px(10.0)).bg(pal.text_2).into_any_element(),
                    "Everything else",
                    format_bytes(used - scanned),
                ),
                (
                    div()
                        .size(px(10.0))
                        .bg(pal.dot)
                        .border_1()
                        .border_color(pal.hairline_strong)
                        .into_any_element(),
                    "Free",
                    format_bytes(v.volume_free_bytes),
                ),
            ];
            (
                parts,
                legend,
                Some(bytes_parts(v.volume_free_bytes)),
                format!("{} on this Mac", format_bytes(v.volume_total_bytes)),
            )
        }
    };
    let has_free = free_fig.is_some();
    let (fig, unit) = free_fig.unwrap_or_else(|| ("--".to_string(), "GB"));
    let mut off = pal.accent;
    off.a = 0.13;

    card()
        .child(card_head(
            "Disk",
            None,
            Some(mono_label(&total_words, pal.text_2).into_any_element()),
        ))
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap(px(S5))
                .child(
                    div()
                        .relative()
                        .size(px(d))
                        .flex_shrink_0()
                        .child(ring_gauge(d, 14.0, parts, pal.dot, pal.hairline_strong))
                        .child(
                            div()
                                .absolute()
                                .top_0()
                                .left_0()
                                .size_full()
                                .flex()
                                .flex_col()
                                .items_center()
                                .justify_center()
                                .gap(px(6.0))
                                .child(dot_number(
                                    &fig,
                                    22.0,
                                    if has_free { pal.accent } else { pal.text_2 },
                                    off,
                                ))
                                .child(mono_label(&format!("{unit} free"), pal.text_2)),
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(180.0))
                        .flex()
                        .flex_col()
                        .gap(px(S2))
                        .when(legend.is_empty(), |el| el.child(pending(frame)))
                        .children(legend.into_iter().map(|(swatch, label, value)| {
                            div()
                                .h(px(30.0))
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(px(S2))
                                .border_b_1()
                                .border_color(pal.hairline)
                                .child(swatch)
                                .child(
                                    div()
                                        .flex_1()
                                        .font(body(FontWeight::MEDIUM))
                                        .text_size(px(13.5))
                                        .text_color(pal.text)
                                        .child(label),
                                )
                                .child(figure_cell(value, 84.0, true))
                        })),
                ),
        )
}

fn file_types_card(frame: &Frame) -> Div {
    let pal = p();
    let head = card_head(
        "By file type",
        frame.current_disk().map(|d| format_bytes(d.scanned_bytes)),
        None,
    );
    let Some(view) = frame.view() else {
        return card().child(head).child(pending(frame));
    };
    if view.categories_ranked.is_empty() {
        return card()
            .child(head)
            .child(empty("No readable files in this folder."));
    }
    let cats = &view.snap.categories;
    let top = view
        .categories_ranked
        .first()
        .map(|&i| cats[i].bytes)
        .unwrap_or(1)
        .max(1);

    card()
        .child(head)
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(10.0))
                .children(view.categories_ranked.iter().map(|&i| {
                    let c = &cats[i];
                    let color = category_color(c.category);
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(5.0))
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(px(S2))
                                .child(
                                    div()
                                        .size(px(8.0))
                                        .flex_shrink_0()
                                        .rounded_full()
                                        .bg(color.swatch),
                                )
                                .child(
                                    line(c.label.clone(), FontWeight::MEDIUM, 13.5, pal.text)
                                        .flex_1(),
                                )
                                .child(figure_cell(format!("{:.0}%", c.share_pct), 40.0, false))
                                .child(figure_cell(format_bytes(c.bytes), 76.0, true)),
                        )
                        .child(led_meter(
                            Some(ratio(c.bytes, top)),
                            color.swatch,
                            pal.dot,
                            4.0,
                        ))
                })),
        )
}
