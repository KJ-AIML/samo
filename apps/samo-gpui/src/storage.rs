//! Storage: a map of the biggest things in the folder, coloured by kind. Click a folder to go inside it (a
//! recent folder opens at once; a new one is measured), "Up" to come back out. Point at a circle to see what it
//! is and what you can do with it. The largest files are listed under the map.

use crate::art::{clip_text_aligned, dither_frame, dot_number, measure_grid};
use crate::components::*;
use crate::disk_view::DiskView;
use crate::overview::{figure_cell, list_row};
use crate::shell::{Head, Page, pending, scan_sub};
use crate::theme::*;
use crate::{Frame, SamoWorkspaceApp};
use gpui::{
    AnyElement, Context, Div, FontWeight, IntoElement, ParentElement, SharedString, Styled, div,
    prelude::*, px,
};
use samo_primitives::{FileCategory, PackedBall};
use std::path::Path;

/// The map's square, sized to the room it has.
fn map_size(frame: &Frame) -> f32 {
    let l = frame.layout;
    let beside = if l.wide() { 300.0 + S5 } else { 0.0 };
    (l.content_w - beside - 2.0 * 20.0 - 2.0 * S4).clamp(220.0, 460.0)
}

/// Fits ball centres/radii from domain space into a `size`-px square.
/// Returns `(left, top, diameter)` per ball.
pub fn fit_balls(balls: &[PackedBall], size: f32) -> Vec<(f32, f32, f32)> {
    if balls.is_empty() {
        return Vec::new();
    }
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for b in balls {
        min_x = min_x.min(b.x - b.r);
        min_y = min_y.min(b.y - b.r);
        max_x = max_x.max(b.x + b.r);
        max_y = max_y.max(b.y + b.r);
    }
    let span = (max_x - min_x).max(max_y - min_y).max(1.0);
    let scale = f64::from(size) / span;
    let off_x = (f64::from(size) - (max_x - min_x) * scale) / 2.0;
    let off_y = (f64::from(size) - (max_y - min_y) * scale) / 2.0;
    balls
        .iter()
        .map(|b| {
            let d = (b.r * 2.0 * scale) as f32;
            let left = ((b.x - b.r - min_x) * scale + off_x) as f32;
            let top = ((b.y - b.r - min_y) * scale + off_y) as f32;
            (left, top, d)
        })
        .collect()
}

/// The folder above `path`, if there is one.
fn parent_of(path: &str) -> Option<String> {
    let parent = Path::new(path).parent()?.to_string_lossy().into_owned();
    Some(if parent.is_empty() {
        "/".to_string()
    } else {
        parent
    })
}

/// `179486` → `179k`, `1240` → `1.2k`, `2300000` → `2.3M`: short enough for the inside of a circle.
fn compact_count(n: u64) -> String {
    match n {
        0..=999 => n.to_string(),
        1_000..=9_999 => format!("{:.1}k", n as f64 / 1_000.0),
        10_000..=999_999 => format!("{}k", n / 1_000),
        _ => format!("{:.1}M", n as f64 / 1_000_000.0),
    }
}

/// A circle's size, as much of it as fits in `room` points of 11.5 pt Plex Mono (every glyph 0.6 em wide):
/// `103.2 MB`, else `103 MB`, else nothing — never a figure cut through.
fn size_words(bytes: u64, room: f32) -> Option<String> {
    const ADVANCE: f32 = 11.5 * 0.6;
    let fits = |s: &str| s.chars().count() as f32 * ADVANCE <= room;
    let (value, unit) = bytes_parts(bytes);
    let full = format!("{value} {unit}");
    if fits(&full) {
        return Some(full);
    }
    let rounded = format!("{:.0} {unit}", value.parse::<f64>().ok()?);
    fits(&rounded).then_some(rounded)
}

fn share_words(part: u64, whole: u64) -> String {
    let pct = ratio(part, whole) * 100.0;
    if pct >= 10.0 {
        format!("{pct:.0}%")
    } else {
        format!("{pct:.1}%")
    }
}

impl SamoWorkspaceApp {
    pub(crate) fn render_storage(&self, frame: &Frame, cx: &mut Context<Self>) -> Page {
        let pal = p();
        let sub = scan_sub(frame, |d| {
            let unreadable = if d.unreadable_paths > 0 {
                format!(
                    ", {} paths Samo couldn’t read",
                    format_count(d.unreadable_paths)
                )
            } else {
                String::new()
            };
            format!(
                "{} in this folder; click a folder to go inside it{}.",
                format_bytes(d.scanned_bytes),
                unreadable
            )
        });

        let up = parent_of(&frame.scan_root).filter(|_| frame.scan_root != "/");
        let head_trailing = up.map(|parent| {
            let name = crate::locations::name_for(&self.locations, &parent);
            button(
                "map-up",
                format!("Up to {name}"),
                Some("arrow-up"),
                Variant::Link,
                true,
            )
            .on_click(cx.listener(move |this, _, _, cx| this.trigger_scan(parent.clone(), cx)))
            .into_any_element()
        });

        let wide = frame.layout.wide();
        let size = map_size(frame);
        let well = |content: AnyElement| {
            div()
                .relative()
                .flex_1()
                .min_w(px(0.0))
                .w_full()
                .h(px(size + 2.0 * S4))
                .rounded(px(R_CARD - 2.0))
                .bg(pal.stage)
                .border_1()
                .border_color(pal.hairline)
                .overflow_hidden()
                .flex()
                .items_center()
                .justify_center()
                .child(measure_grid(size + 2.0 * S4))
                .child(content)
        };

        let body: Div = match frame.view() {
            Some(view) if view.snap.balls.is_empty() => div().child(empty(
                "Nothing to map: this folder is empty, or Samo can’t read what is in it.",
            )),
            Some(view) => div()
                .flex()
                .when(wide, |el| el.flex_row())
                .when(!wide, |el| el.flex_col())
                .gap(px(S5))
                .child(well(self.ball_map(view, size, cx).into_any_element()))
                .child(
                    div()
                        .when(wide, |el| el.w(px(300.0)).flex_shrink_0())
                        .child(self.ball_detail(view, cx)),
                ),
            // Keep the map's room while the next folder is measured, so nothing jumps.
            None => div().child(well(
                div().px(px(S5)).child(pending(frame)).into_any_element(),
            )),
        };

        let map_card = card()
            .child(card_head(
                "Space map",
                frame.view().map(|v| v.snap.balls.len().to_string()),
                head_trailing,
            ))
            .child(body);

        Page {
            head: Head {
                icon: "bubbles",
                title: "Storage".to_string(),
                sub,
                crumbs: true,
                actions: self.disk_actions(frame, cx),
            },
            body: div()
                .flex()
                .flex_col()
                .gap(px(S5))
                .child(map_card)
                .child(self.files_card(frame, cx)),
            fill: false,
        }
    }

    fn ball_map(&self, view: &DiskView, map: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = p();
        let disk = view.snap.as_ref();
        let placed = fit_balls(&disk.balls, map);

        div().relative().size(px(map)).flex_shrink_0().children(
            disk.balls
                .iter()
                .zip(placed)
                .enumerate()
                .map(|(i, (ball, (left, top, d)))| {
                    let color = category_color(ball.category);
                    let focused = self.hovered_ball.as_deref() == Some(ball.path.as_str());
                    let suggested = view.is_suggestion(&ball.path);
                    let folder = view.is_folder(&ball.path);
                    let hover_path = ball.path.clone();
                    let open_path = ball.path.clone();
                    let d = d.max(6.0);
                    let files = disk
                        .largest_folders
                        .iter()
                        .find(|f| f.path == ball.path)
                        .map(|f| f.file_count);

                    div()
                        .id(SharedString::from(format!("ball-{i}")))
                        .absolute()
                        .left(px(left))
                        .top(px(top))
                        .size(px(d))
                        .rounded_full()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(px(2.0))
                        .cursor_pointer()
                        .bg(if focused {
                            color.fill_strong
                        } else {
                            color.fill
                        })
                        .border_color(color.swatch)
                        .when(focused, |el| el.border_2())
                        .when(!focused, |el| el.border_1())
                        .hover(move |s| s.bg(color.fill_strong))
                        .tooltip({
                            let words = SharedString::from(if folder {
                                format!("{} — click to go inside", ball.label)
                            } else {
                                ball.label.clone()
                            });
                            move |_, cx| tip(words.clone(), cx)
                        })
                        .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                            // The last circle pointed at stays shown, so its actions can be reached.
                            if *hovered && this.hovered_ball.as_deref() != Some(hover_path.as_str())
                            {
                                this.hovered_ball = Some(hover_path.clone());
                                cx.notify();
                            }
                        }))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if folder {
                                this.trigger_scan(open_path.clone(), cx);
                            }
                        }))
                        .when(d >= 92.0 && folder, |el| {
                            el.child(icon("folder", 14.0, color.swatch))
                        })
                        .when(d >= 56.0, |el| {
                            el.child(
                                div().w(px(d * 0.76)).flex().justify_center().child(
                                    clip_text_aligned(
                                        ball.label.clone(),
                                        body(FontWeight::MEDIUM),
                                        if d >= 120.0 { 14.0 } else { 12.5 },
                                        pal.text,
                                        true,
                                    )
                                    .w_full(),
                                ),
                            )
                            .children(
                                size_words(ball.bytes, d * 0.78).map(|size| {
                                    div()
                                        .whitespace_nowrap()
                                        .font(mono(FontWeight::MEDIUM))
                                        .text_size(px(11.5))
                                        .text_color(pal.text_2)
                                        .child(size)
                                }),
                            )
                        })
                        // A third line only where the circle is wide enough to hold it, and never wider.
                        .when(d >= 150.0, |el| {
                            el.child(
                                div().w(px(d * 0.66)).flex().justify_center().child(
                                    clip_text_aligned(
                                        match files {
                                            Some(n) => format!(
                                                "{} · {} files",
                                                share_words(ball.bytes, disk.scanned_bytes),
                                                compact_count(n)
                                            ),
                                            None => share_words(ball.bytes, disk.scanned_bytes),
                                        },
                                        mono(FontWeight::NORMAL),
                                        11.0,
                                        pal.text_2,
                                        true,
                                    )
                                    .w_full(),
                                ),
                            )
                        })
                        .when(suggested, |el| el.child(dither_frame(d / 2.0, pal.accent)))
                }),
        )
    }

    fn ball_detail(&self, view: &DiskView, cx: &mut Context<Self>) -> AnyElement {
        let pal = p();
        let disk = view.snap.as_ref();
        let pointed = self
            .hovered_ball
            .as_deref()
            .and_then(|path| disk.balls.iter().find(|b| b.path == path));

        let Some(ball) = pointed else {
            // Nothing pointed at yet: what this folder is made of, in the map's colours.
            return div()
                .flex()
                .flex_col()
                .gap(px(S3))
                .child(mono_label("In this folder", pal.text_2))
                .child(
                    div().flex().flex_col().children(
                        CATEGORY_ORDER
                            .iter()
                            .filter_map(|cat| disk.categories.iter().find(|c| c.category == *cat))
                            .filter(|c| c.bytes > 0)
                            .map(|c| {
                                div()
                                    .h(px(34.0))
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap(px(S2))
                                    .border_b_1()
                                    .border_color(pal.hairline)
                                    .child(
                                        div()
                                            .size(px(10.0))
                                            .flex_shrink_0()
                                            .rounded_full()
                                            .bg(category_color(c.category).swatch),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .text_size(px(13.5))
                                            .text_color(pal.text)
                                            .child(c.label.clone()),
                                    )
                                    .child(figure_cell(
                                        share_words(c.bytes, disk.scanned_bytes),
                                        46.0,
                                        false,
                                    ))
                                    .child(figure_cell(format_bytes(c.bytes), 76.0, true))
                            }),
                    ),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(S2))
                        .child(
                            div()
                                .relative()
                                .size(px(16.0))
                                .flex_shrink_0()
                                .rounded_full()
                                .bg(pal.card)
                                .child(dither_frame(8.0, pal.accent)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .text_size(px(T_META))
                                .text_color(pal.text_2)
                                .child("A dithered ring is something Samo suggests clearing."),
                        ),
                )
                .child(
                    div()
                        .text_size(px(T_META))
                        .line_height(px(18.0))
                        .text_color(pal.text_2)
                        .child(
                            "Point at a circle to see what it is. Click a folder to go inside it.",
                        ),
                )
                .into_any_element();
        };

        let is_folder = view.is_folder(&ball.path);
        let suggested = view.is_suggestion(&ball.path);
        let color = category_color(ball.category);
        let files = disk
            .largest_folders
            .iter()
            .find(|f| f.path == ball.path)
            .map(|f| f.file_count);
        let open_path = ball.path.clone();
        let reveal_path = ball.path.clone();
        let trash_path = ball.path.clone();
        let label = ball.label.clone();
        let bytes = ball.bytes;
        let identity = ball.identity;
        let (figure, unit) = bytes_parts(ball.bytes);
        let mut off = pal.accent;
        off.a = 0.13;

        div()
            .flex()
            .flex_col()
            .gap(px(S4))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap(px(S2))
                    .child(div().size(px(10.0)).rounded_full().bg(color.swatch))
                    .child(tag(ball.category.label(), TagKind::Neutral, false))
                    .child(tag(
                        if is_folder { "Folder" } else { "File" },
                        TagKind::Neutral,
                        false,
                    ))
                    .when(suggested, |el| {
                        el.child(tag("Samo suggests", TagKind::Accent, true))
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(S1))
                    .child(h2(ball.label.clone()))
                    .child(
                        div()
                            .font(mono(FontWeight::NORMAL))
                            .text_size(px(12.0))
                            .line_height(px(17.0))
                            .text_color(pal.text_2)
                            .child(display_path(&ball.path)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_end()
                    .gap(px(S2))
                    .child(dot_number(&figure, 28.0, pal.accent, off))
                    .child(
                        div()
                            .font(mono(FontWeight::MEDIUM))
                            .text_size(px(T_META))
                            .text_color(pal.text_2)
                            .child(unit),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(kv_row(
                        "Share of this folder",
                        share_words(ball.bytes, disk.scanned_bytes),
                    ))
                    .when_some(files, |el, n| {
                        el.child(kv_row("Files inside", format_count(n)))
                    })
                    .child(kv_row("Kind", kind_words(ball.category).to_string())),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap(px(S2))
                    .when(is_folder, |el| {
                        el.child(
                            button(
                                "ball-open",
                                "Go inside",
                                Some("folder"),
                                Variant::Default,
                                true,
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| this.trigger_scan(open_path.clone(), cx),
                            )),
                        )
                    })
                    .child(
                        icon_button(
                            "ball-reveal",
                            "reveal",
                            "Reveal in Finder",
                            Variant::Default,
                            true,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.reveal_in_finder(reveal_path.clone(), cx)
                        })),
                    )
                    .child(
                        button(
                            "ball-trash",
                            "Move to Trash…",
                            Some("trash"),
                            Variant::Default,
                            true,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.request_trash_confirmation(
                                label.clone(),
                                trash_path.clone(),
                                bytes,
                                identity,
                                cx,
                            )
                        })),
                    ),
            )
            .into_any_element()
    }

    fn files_card(&self, frame: &Frame, cx: &mut Context<Self>) -> Div {
        let pal = p();
        let head = card_head(
            "Largest files",
            frame
                .current_disk()
                .map(|d| d.largest_files.len().min(25).to_string()),
            None,
        );
        let Some(disk) = frame.current_disk() else {
            return card().child(head).child(pending(frame));
        };
        if disk.largest_files.is_empty() {
            return card()
                .child(head)
                .child(empty("No readable files in this folder."));
        }

        card().child(head).child(
            div()
                .flex()
                .flex_col()
                .children(
                    disk.largest_files
                        .iter()
                        .take(25)
                        .enumerate()
                        .map(|(i, f)| {
                            let reveal_path = f.path.clone();
                            let trash_path = f.path.clone();
                            let name = f.name.clone();
                            let bytes = f.bytes;
                            let identity = f.identity;
                            let parent = Path::new(&f.path)
                                .parent()
                                .map(|p| display_path(&p.to_string_lossy()))
                                .unwrap_or_default();

                            list_row(SharedString::from(format!("file-{i}")), "file-row")
                                .child(
                                    div()
                                        .size(px(8.0))
                                        .flex_shrink_0()
                                        .rounded_full()
                                        .bg(category_color(f.category).swatch),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .py(px(S2))
                                        .flex()
                                        .flex_col()
                                        .gap(px(S1))
                                        .child(
                                            line(
                                                f.name.clone(),
                                                FontWeight::MEDIUM,
                                                T_BODY,
                                                pal.text,
                                            )
                                            .w_full(),
                                        )
                                        .child(mono_line(parent, 12.0, pal.text_2).w_full()),
                                )
                                .when(frame.layout.roomy(), |el| {
                                    el.child(tag(f.category.label(), TagKind::Neutral, false))
                                })
                                .child(figure_cell(format_bytes(f.bytes), 84.0, true))
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .gap(px(2.0))
                                        .opacity(0.0)
                                        .group_hover("file-row", |s| s.opacity(1.0))
                                        .child(
                                            icon_button(
                                                SharedString::from(format!("file-reveal-{i}")),
                                                "reveal",
                                                "Reveal in Finder",
                                                Variant::Link,
                                                true,
                                            )
                                            .on_click(
                                                cx.listener(move |this, _, _, cx| {
                                                    this.reveal_in_finder(reveal_path.clone(), cx)
                                                }),
                                            ),
                                        )
                                        .child(
                                            icon_button(
                                                SharedString::from(format!("file-trash-{i}")),
                                                "trash",
                                                "Move to Trash…",
                                                Variant::Link,
                                                true,
                                            )
                                            .on_click(
                                                cx.listener(move |this, _, _, cx| {
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

/// What a category means, in a few words.
fn kind_words(cat: FileCategory) -> &'static str {
    match cat {
        FileCategory::Code => "source code and build output",
        FileCategory::Documents => "documents, notes and data",
        FileCategory::Archives => "archives and disk images",
        FileCategory::Media => "photos, video and audio",
        FileCategory::System => "system and library files",
        FileCategory::Applications => "apps and app bundles",
        FileCategory::Other => "everything else",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use samo_primitives::FileNodeIdentity;

    #[test]
    fn a_circles_size_shortens_to_fit_and_is_never_cut() {
        const MB: u64 = 1_048_576;
        let size = (103.2 * MB as f64) as u64;
        assert_eq!(size_words(size, 120.0).as_deref(), Some("103.2 MB"));
        // Eight glyphs don't fit in 50 pt; six do.
        assert_eq!(size_words(size, 50.0).as_deref(), Some("103 MB"));
        assert_eq!(size_words(size, 30.0), None);
    }

    fn ball(x: f64, y: f64, r: f64) -> PackedBall {
        PackedBall {
            id: String::new(),
            label: String::new(),
            path: String::new(),
            bytes: 1,
            category: FileCategory::Other,
            x,
            y,
            r,
            identity: FileNodeIdentity {
                device_id: 0,
                inode: 0,
            },
        }
    }

    #[test]
    fn fitted_balls_stay_inside_the_square() {
        let balls = vec![
            ball(500.0, 500.0, 200.0),
            ball(820.0, 500.0, 100.0),
            ball(-40.0, 300.0, 60.0),
        ];
        for (left, top, d) in fit_balls(&balls, 400.0) {
            assert!(left >= -0.01 && top >= -0.01);
            assert!(left + d <= 400.01 && top + d <= 400.01);
        }
    }

    #[test]
    fn single_ball_fills_the_square() {
        let placed = fit_balls(&[ball(500.0, 500.0, 260.0)], 400.0);
        assert_eq!(placed.len(), 1);
        let (left, top, d) = placed[0];
        assert!(left.abs() < 0.01 && top.abs() < 0.01 && (d - 400.0).abs() < 0.01);
    }

    #[test]
    fn up_goes_one_folder_out_and_stops_at_the_volume() {
        assert_eq!(parent_of("/Users/a/Developer").as_deref(), Some("/Users/a"));
        assert_eq!(parent_of("/Users").as_deref(), Some("/"));
        assert_eq!(parent_of("/"), None);
    }

    #[test]
    fn counts_get_short_inside_circles() {
        assert_eq!(compact_count(42), "42");
        assert_eq!(compact_count(1_240), "1.2k");
        assert_eq!(compact_count(179_486), "179k");
        assert_eq!(compact_count(2_300_000), "2.3M");
    }

    #[test]
    fn shares_keep_one_decimal_only_when_small() {
        assert_eq!(share_words(1, 3), "33%");
        assert_eq!(share_words(1, 200), "0.5%");
        assert_eq!(share_words(1, 0), "0.0%");
    }
}
