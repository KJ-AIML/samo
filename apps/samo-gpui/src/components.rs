//! Samo's components: buttons, cards, shells, tags, counts, the face switch, and the empty / loading /
//! error lines with Samo's probe. Talvyn-family behaviour (one Ink action per view, Signal only for what
//! can't be undone), at Samo's denser instrument scale.

use crate::art::{Mood, Who, cell, clip_text, clock_ms};
use crate::theme::*;
use gpui::{
    AnyElement, AnyView, App, BoxShadow, Context, Div, ElementId, FontWeight, Hsla, IntoElement,
    ParentElement, Render, SharedString, Stateful, Styled, Svg, Window, div, point, prelude::*, px,
    svg,
};

pub fn icon(name: &str, size: f32, color: Hsla) -> Svg {
    svg()
        .path(SharedString::from(format!("icons/{name}.svg")))
        .size(px(size))
        .flex_shrink_0()
        .text_color(color)
}

fn shadow(y: f32, blur: f32, spread: f32, color: Hsla) -> BoxShadow {
    BoxShadow {
        color,
        offset: point(px(0.0), px(y)),
        blur_radius: px(blur),
        spread_radius: px(spread),
    }
}

/// `--shadow-card`: soft, tinted with Ink, never black on white.
pub fn shadow_card() -> Vec<BoxShadow> {
    vec![
        shadow(1.0, 2.0, 0.0, p().shade_2),
        shadow(14.0, 30.0, -18.0, p().shade_3),
    ]
}

/// `--shadow-pop`: a dialog, a lifted card.
pub fn shadow_pop() -> Vec<BoxShadow> {
    vec![
        shadow(30.0, 80.0, -24.0, p().shade_4),
        shadow(2.0, 8.0, 0.0, p().shade_1),
    ]
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    /// The one Ink action of a view.
    Primary,
    Default,
    /// Reads in Signal, never filled: only what can't be undone.
    Danger,
    /// Quiet, for rows and the sidebar.
    Link,
}

/// A button. `compact` is for actions inside a dense row; everything else is `CONTROL_H` tall.
pub fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    icon_name: Option<&str>,
    variant: Variant,
    compact: bool,
) -> Stateful<Div> {
    let pal = p();
    let label: SharedString = label.into();
    let has_label = !label.is_empty();
    let (bg, fg, ring, hover_bg, hover_fg) = match variant {
        Variant::Primary => (
            pal.primary_bg,
            pal.primary_fg,
            Some(pal.primary_bg),
            pal.primary_hover,
            pal.primary_fg,
        ),
        Variant::Default => (
            pal.card,
            pal.text,
            Some(pal.hairline_strong),
            pal.card_hover,
            pal.text,
        ),
        Variant::Danger => (
            pal.card,
            pal.signal_text,
            Some(pal.hairline_strong),
            pal.card_hover,
            pal.signal_text,
        ),
        Variant::Link => (
            Hsla::transparent_black(),
            pal.text_2,
            None,
            pal.hover,
            pal.text,
        ),
    };
    let h = if compact {
        CONTROL_H_COMPACT
    } else {
        CONTROL_H
    };
    let pad = match (has_label, variant) {
        (false, _) => 0.0,
        (true, Variant::Link) => S3,
        (true, _) if compact => S3,
        (true, _) => S4,
    };

    div()
        .id(id)
        .h(px(h))
        .when(!has_label, |el| el.w(px(h)))
        .px(px(pad))
        .flex()
        .flex_row()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .gap(px(S2))
        .rounded(px(R_CONTROL))
        .bg(bg)
        .when_some(ring, |el, ring| el.border_1().border_color(ring))
        .when(variant != Variant::Link, |el| {
            el.shadow(vec![shadow(
                1.0,
                2.0,
                0.0,
                if variant == Variant::Primary {
                    pal.shade_3
                } else {
                    pal.shade_1
                },
            )])
        })
        .font(body(FontWeight::MEDIUM))
        .text_size(px(if compact { T_META + 0.5 } else { T_BODY }))
        .text_color(fg)
        .whitespace_nowrap()
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg).text_color(hover_fg))
        .children(icon_name.map(|name| icon(name, if compact { 14.0 } else { 15.0 }, fg)))
        .when(has_label, |el| el.child(label))
}

/// A primary button that is working: three pixels run along its front, and it takes no clicks.
pub fn busy_button(label: &str) -> Div {
    let pal = p();
    let mut bg = pal.primary_bg;
    bg.a = 0.8;
    let step = ((clock_ms() / 300) % 3) as usize;
    div()
        .h(px(CONTROL_H))
        .px(px(S4))
        .flex()
        .flex_row()
        .flex_shrink_0()
        .items_center()
        .gap(px(S2))
        .rounded(px(R_CONTROL))
        .bg(bg)
        .font(body(FontWeight::MEDIUM))
        .text_size(px(T_BODY))
        .text_color(pal.primary_fg)
        .child(
            div()
                .flex()
                .flex_row()
                .gap(px(3.0))
                .mr(px(4.0))
                .children((0..3).map(move |i| {
                    let mut c = pal.primary_fg;
                    if i != step {
                        c.a = 0.0;
                    }
                    div().size(px(4.0)).bg(c)
                })),
        )
        .child(label.to_string())
}

/// Square icon-only button with a tooltip.
pub fn icon_button(
    id: impl Into<ElementId>,
    icon_name: &str,
    tooltip: &str,
    variant: Variant,
    compact: bool,
) -> Stateful<Div> {
    let tip_text = SharedString::from(tooltip.to_string());
    button(id, "", Some(icon_name), variant, compact)
        .tooltip(move |_, cx| tip(tip_text.clone(), cx))
}

pub struct Tooltip(SharedString);

impl Render for Tooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let pal = p();
        div()
            .px(px(S2 + 2.0))
            .py(px(S1 + 1.0))
            .rounded(px(R_CHIP))
            .bg(pal.card)
            .border_1()
            .border_color(pal.hairline_strong)
            .shadow(shadow_pop())
            .font(body(FontWeight::NORMAL))
            .text_size(px(T_META))
            .text_color(pal.text)
            .child(self.0.clone())
    }
}

pub fn tip(text: SharedString, cx: &mut App) -> AnyView {
    cx.new(|_| Tooltip(text)).into()
}

/// A white card with a hairline ring and `--shadow-card`.
pub fn card() -> Div {
    let pal = p();
    div()
        .relative()
        .flex()
        .flex_col()
        .min_w(px(0.0))
        .p(px(20.0))
        .rounded(px(R_CARD))
        .bg(pal.card)
        .border_1()
        .border_color(pal.hairline)
        .shadow(shadow_card())
}

/// A card's head: its `h2`, then (pushed right) a count and anything else that belongs to it.
pub fn card_head(
    title: impl Into<SharedString>,
    count: Option<String>,
    trailing: Option<AnyElement>,
) -> Div {
    div()
        .mb(px(S4))
        .min_h(px(26.0))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(S3))
        .child(h2(title))
        .child(div().flex_1())
        .children(count.map(count_chip))
        .children(trailing)
}

pub fn h1(text: impl Into<SharedString>) -> Div {
    div()
        .font(display())
        .text_size(px(T_H1))
        .line_height(px((T_H1 * 1.1).round()))
        .text_color(p().text)
        .child(text.into())
}

pub fn h2(text: impl Into<SharedString>) -> Div {
    div()
        .flex_shrink_0()
        .whitespace_nowrap()
        .font(display())
        .text_size(px(T_H2))
        .line_height(px((T_H2 * 1.25).round()))
        .text_color(p().text)
        .child(text.into())
}

/// A count beside a card's title: Cobalt wash, mono figures.
pub fn count_chip(text: String) -> Div {
    let pal = p();
    div()
        .h(px(24.0))
        .min_w(px(24.0))
        .px(px(S2))
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .rounded(px(R_CHIP))
        .bg(pal.accent_wash)
        .border_1()
        .border_color(pal.accent_line)
        .font(mono(FontWeight::MEDIUM))
        .text_size(px(12.0))
        .text_color(pal.accent_text)
        .whitespace_nowrap()
        .child(text)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TagKind {
    Neutral,
    Accent,
    Signal,
}

/// A small mono tag; `lamp` adds the square lamp a rack uses to say what a unit is doing.
pub fn tag(text: impl Into<SharedString>, kind: TagKind, lamp: bool) -> Div {
    let pal = p();
    let (bg, ring, fg) = match kind {
        TagKind::Neutral => (pal.shell, pal.hairline, pal.text),
        TagKind::Accent => (pal.accent_wash, pal.accent_line, pal.accent_text),
        TagKind::Signal => (pal.signal_wash, pal.signal_line, pal.signal_text),
    };
    div()
        .h(px(22.0))
        .pl(px(S2))
        .pr(px(10.0))
        .flex()
        .flex_row()
        .flex_shrink_0()
        .items_center()
        .gap(px(6.0))
        .rounded(px(R_CHIP))
        .bg(bg)
        .border_1()
        .border_color(ring)
        .font(mono(FontWeight::MEDIUM))
        .text_size(px(12.0))
        .text_color(fg)
        .whitespace_nowrap()
        .when(lamp, |el| {
            el.child(div().size(px(6.0)).flex_shrink_0().bg(fg))
        })
        .child(text.into())
}

/// A small mono label in capitals (the rail's group names, a figure's name).
pub fn mono_label(text: &str, color: Hsla) -> Div {
    div()
        .font(mono(FontWeight::MEDIUM))
        .text_size(px(T_LABEL))
        .text_color(color)
        .whitespace_nowrap()
        .child(text.to_uppercase())
}

/// An empty list: Samo's mascot asleep beside what this place is for.
pub fn empty(words: &str) -> Div {
    let pal = p();
    div()
        .py(px(S3))
        .pl(px(S3))
        .pr(px(S4))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(S3))
        .rounded(px(R_SHELL - 2.0))
        .bg(pal.shell)
        .border_1()
        .border_color(pal.hairline)
        .child(cell(Who::Mascot, Mood::Asleep, 1.5))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .font(body(FontWeight::NORMAL))
                .text_size(px(T_BODY))
                .line_height(px(20.0))
                .text_color(pal.text_2)
                .child(words.to_string()),
        )
}

/// Something Samo is doing: the scanner at work, sweeping its visor.
pub fn loading(words: &str) -> Div {
    div()
        .min_h(px(36.0))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(S3))
        .child(cell(Who::Samo, Mood::Working, 1.5))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .font(body(FontWeight::NORMAL))
                .text_size(px(T_BODY))
                .text_color(p().text_2)
                .child(words.to_string()),
        )
}

/// Something went wrong: the mascot with X eyes, and the words (saying how to recover) in Signal.
pub fn error_line(words: &str) -> Div {
    div()
        .min_h(px(36.0))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(S3))
        .child(cell(Who::Mascot, Mood::Blocked, 1.5))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .font(body(FontWeight::NORMAL))
                .text_size(px(T_BODY))
                .line_height(px(20.0))
                .text_color(p().signal_text)
                .child(words.to_string()),
        )
}

/// Label + figure in a quiet list.
pub fn kv_row(label: &str, value: String) -> Div {
    let pal = p();
    div()
        .h(px(34.0))
        .whitespace_nowrap()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(S3))
        .border_b_1()
        .border_color(pal.hairline)
        .child(
            div()
                .flex_shrink_0()
                .font(body(FontWeight::NORMAL))
                .text_size(px(T_BODY))
                .text_color(pal.text_2)
                .child(label.to_string()),
        )
        .child(
            div()
                .font(mono(FontWeight::NORMAL))
                .text_size(px(T_META))
                .text_color(pal.text)
                .child(value),
        )
}

/// The track of a segmented switch (a time span, a sort): its `segment`s sit in a recessed well.
pub fn segments(items: impl IntoIterator<Item = Stateful<Div>>) -> Div {
    let pal = p();
    div()
        .h(px(28.0))
        .p(px(2.0))
        .flex()
        .flex_row()
        .flex_shrink_0()
        .gap(px(2.0))
        .rounded(px(R_CONTROL))
        .bg(pal.shell)
        .border_1()
        .border_color(pal.hairline)
        .children(items)
}

/// One choice of a segmented switch; the picked one is raised onto the card.
pub fn segment(id: impl Into<ElementId>, label: &str, on: bool) -> Stateful<Div> {
    let pal = p();
    div()
        .id(id)
        .h_full()
        .px(px(10.0))
        .flex()
        .items_center()
        .rounded(px(R_CHIP))
        .font(mono(FontWeight::MEDIUM))
        .text_size(px(12.0))
        .when(on, |el| {
            el.bg(pal.card)
                .text_color(pal.text)
                .shadow(vec![shadow(1.0, 2.0, 0.0, pal.shade_1)])
        })
        .when(!on, |el| {
            el.text_color(pal.text_2)
                .cursor_pointer()
                .hover(|s| s.text_color(pal.text))
        })
        .child(label.to_string())
}

/// A section's name over a hairline, with what it's about at the right: `THIS MAC · LIVE ——— up 5d 6h`.
pub fn section_rule(label: &str, about: Option<String>) -> Div {
    let pal = p();
    div()
        .h(px(20.0))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(S3))
        .child(div().flex_shrink_0().child(mono_label(label, pal.text_2)))
        .child(div().flex_1().h(px(1.0)).bg(pal.hairline_strong))
        .children(about.map(|a| {
            div()
                .flex_shrink_0()
                .font(mono(FontWeight::NORMAL))
                .text_size(px(T_LABEL))
                .text_color(pal.text_2)
                .child(a)
        }))
}

/// The switch: its thumb is a little face, asleep when off and awake (with a smile) when on, Cobalt under it.
pub fn face_switch(id: impl Into<ElementId>, on: bool) -> Stateful<Div> {
    let pal = p();
    let face = pal.thumb_face;
    let eye = |x: f32, y: f32, w: f32, h: f32| {
        div()
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(w))
            .h(px(h))
            .bg(face)
    };
    let thumb = div()
        .relative()
        .size(px(20.0))
        .rounded_full()
        .bg(pal.thumb)
        .shadow(vec![shadow(1.0, 2.0, 0.0, pal.shade_1)])
        .when(!on, |el| {
            el.child(eye(5.0, 9.0, 4.0, 2.0))
                .child(eye(11.0, 9.0, 4.0, 2.0))
        })
        .when(on, |el| {
            el.child(eye(5.0, 4.0, 2.0, 4.0))
                .child(eye(13.0, 4.0, 2.0, 4.0))
                .child(eye(5.0, 10.0, 2.0, 2.0))
                .child(eye(13.0, 10.0, 2.0, 2.0))
                .child(eye(7.0, 12.0, 6.0, 2.0))
        });
    div()
        .id(id)
        .w(px(40.0))
        .h(px(24.0))
        .p(px(2.0))
        .flex()
        .flex_row()
        .flex_shrink_0()
        .when(on, |el| el.justify_end())
        .rounded_full()
        .bg(if on { pal.accent } else { pal.field_line })
        .cursor_pointer()
        .child(thumb)
}

/// One line of body text that ends in "…" when it doesn't fit. It has no width of its own: give it
/// `.flex_1()` to fill a row, or `.w_full()` in a column.
pub fn line(
    text: impl Into<SharedString>,
    weight: FontWeight,
    size: f32,
    color: Hsla,
) -> gpui::Canvas<()> {
    clip_text(text, body(weight), size, color)
}

/// The same, in mono (paths).
pub fn mono_line(text: impl Into<SharedString>, size: f32, color: Hsla) -> gpui::Canvas<()> {
    clip_text(text, mono(FontWeight::NORMAL), size, color)
}
