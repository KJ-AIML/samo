//! An extension's page, drawn from the widgets its helper sends (ADR-002 E3) with Samo's own components, so every
//! extension looks like Samo. A section rule splits the page; what follows a rule sits in one card. Figures and
//! meters sit side by side; rows, switches, buttons and notes stack.
//!
//! The rules hold here too: unknown is `--`, never `0`; only an error is Signal (waiting for you isn't wrong);
//! there is no dither (an extension's figures are measured or reported, never Samo's suggestion); and an
//! extension's buttons are never the view's one Ink action.

use crate::SamoWorkspaceApp;
use crate::art::{dot_number, led_meter};
use crate::components::*;
use crate::theme::*;
use gpui::{
    AnyElement, Context, Div, FontWeight, Hsla, IntoElement, ParentElement, SharedString, Styled,
    div, prelude::*, px,
};
use samo_ext_protocol::{ButtonKind, Lamp, NoteKind, RowItem, Widget};
use samo_extensions::ExtSlot;
use samo_primitives::Reading;

/// A figure in dot-matrix digits and the words after it: `("1.23", "M tokens")`. Unknown is `--`.
pub fn figure_parts(value: &Reading<f64>, unit: &str) -> (String, String) {
    let suffix = |scale: &str| match (scale.is_empty(), unit.is_empty()) {
        (true, _) => unit.to_string(),
        (false, true) => scale.to_string(),
        (false, false) => format!("{scale} {unit}"),
    };
    match value {
        Reading::Available { value, .. } if value.is_finite() => {
            let v = *value;
            let a = v.abs();
            if a < 10_000.0 {
                (format!("{v:.0}"), suffix(""))
            } else if a < 1_000_000.0 {
                (format!("{:.1}", v / 1_000.0), suffix("k"))
            } else if a < 1_000_000_000.0 {
                (format!("{:.2}", v / 1_000_000.0), suffix("M"))
            } else {
                (format!("{:.2}", v / 1_000_000_000.0), suffix("G"))
            }
        }
        _ => ("--".to_string(), unit.to_string()),
    }
}

/// A meter's share, `0..=1`, or `None` (lights nothing) when unknown.
pub fn meter_share(fraction: &Reading<f64>) -> Option<f32> {
    match fraction {
        Reading::Available { value, .. } if value.is_finite() => Some(value.clamp(0.0, 1.0) as f32),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// Cobalt: busy.
    Accent,
    /// Ink: waiting for you.
    Ink,
    /// Grey: done or off.
    Quiet,
    /// Signal: something is wrong. Only an error.
    Signal,
}

pub fn lamp_tone(lamp: Lamp) -> Tone {
    match lamp {
        Lamp::Working => Tone::Accent,
        Lamp::NeedsYou => Tone::Ink,
        Lamp::Done | Lamp::Off => Tone::Quiet,
        Lamp::Error => Tone::Signal,
    }
}

fn tone_color(tone: Tone) -> Hsla {
    let pal = p();
    match tone {
        Tone::Accent => pal.accent,
        Tone::Ink => pal.text,
        Tone::Quiet => pal.text_2,
        Tone::Signal => pal.signal,
    }
}

/// One lamp, the shape of a rack's: a square, or `!` when something waits for you.
fn lamp_mark(lamp: Lamp) -> AnyElement {
    let color = tone_color(lamp_tone(lamp));
    if lamp == Lamp::NeedsYou {
        return div()
            .w(px(10.0))
            .flex_shrink_0()
            .font(mono(FontWeight::BOLD))
            .text_size(px(T_BODY))
            .text_color(color)
            .child("!")
            .into_any_element();
    }
    div()
        .w(px(10.0))
        .flex_shrink_0()
        .flex()
        .justify_center()
        .child(
            div()
                .size(px(7.0))
                .when(lamp == Lamp::Off, |el| el.border_1().border_color(color))
                .when(lamp != Lamp::Off, |el| el.bg(color)),
        )
        .into_any_element()
}

fn row_line(item: &RowItem) -> Div {
    let pal = p();
    let error = item.lamp == Lamp::Error;
    div()
        .min_h(px(44.0))
        .py(px(6.0))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(S3))
        .border_b_1()
        .border_color(pal.hairline)
        .child(lamp_mark(item.lamp))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(line(item.title.clone(), FontWeight::MEDIUM, T_BODY, pal.text).w_full())
                .when(!item.detail.is_empty(), |el| {
                    el.child(
                        line(
                            item.detail.clone(),
                            FontWeight::NORMAL,
                            T_META,
                            if error { pal.signal_text } else { pal.text_2 },
                        )
                        .w_full(),
                    )
                }),
        )
        .children(item.tag.clone().map(|t| tag(t, TagKind::Neutral, false)))
        .when(!item.trailing.is_empty(), |el| {
            el.child(
                div()
                    .flex_shrink_0()
                    .font(mono(FontWeight::NORMAL))
                    .text_size(px(T_META))
                    .text_color(pal.text)
                    .child(item.trailing.clone()),
            )
        })
}

/// A figure or meter cell, side by side with its neighbours.
fn cell_frame() -> Div {
    let pal = p();
    div()
        .flex_1()
        .min_w(px(170.0))
        .p(px(S3))
        .rounded(px(R_CONTROL + 2.0))
        .bg(pal.shell)
        .border_1()
        .border_color(pal.hairline)
        .flex()
        .flex_col()
        .gap(px(S2))
}

fn note_text(note: Option<&String>) -> Option<Div> {
    let pal = p();
    note.filter(|n| !n.is_empty()).map(|n| {
        div()
            .font(body(FontWeight::NORMAL))
            .text_size(px(T_META))
            .line_height(px(17.0))
            .text_color(pal.text_2)
            .child(n.clone())
    })
}

fn figure_cell(label: &str, value: &Reading<f64>, unit: &str, note: Option<&String>) -> Div {
    let pal = p();
    let (digits, words) = figure_parts(value, unit);
    let known = matches!(value, Reading::Available { .. });
    let mut faint = pal.accent;
    faint.a = 0.13;
    cell_frame()
        .child(mono_label(label, pal.text_2))
        .child(
            div()
                .flex()
                .flex_row()
                .items_end()
                .gap(px(6.0))
                .child(dot_number(
                    &digits,
                    20.0,
                    if known { pal.accent } else { pal.text_2 },
                    faint,
                ))
                .when(!words.is_empty(), |el| {
                    el.child(
                        div()
                            .font(mono(FontWeight::MEDIUM))
                            .text_size(px(12.0))
                            .text_color(pal.text_2)
                            .child(words),
                    )
                }),
        )
        .children(note_text(note))
}

fn meter_cell(label: &str, fraction: &Reading<f64>, note: Option<&String>) -> Div {
    let pal = p();
    let share = meter_share(fraction);
    cell_frame()
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .child(div().flex_1().child(mono_label(label, pal.text_2)))
                .child(
                    div()
                        .font(mono(FontWeight::MEDIUM))
                        .text_size(px(12.0))
                        .text_color(pal.text)
                        .child(
                            share
                                .map_or_else(|| "--".to_string(), |s| format!("{:.0}%", s * 100.0)),
                        ),
                ),
        )
        .child(led_meter(share, pal.accent, pal.dot, 6.0))
        .children(note_text(note))
}

impl SamoWorkspaceApp {
    /// The widgets an extension sent, as Samo draws them.
    pub(crate) fn render_ext_widgets(
        &self,
        slot: ExtSlot,
        view: &[Widget],
        cx: &mut Context<Self>,
    ) -> Div {
        let mut page = div().flex().flex_col().gap(px(S4));
        let mut card_items: Vec<AnyElement> = Vec::new();
        let mut cells: Vec<Div> = Vec::new();

        let flush_cells = |cells: &mut Vec<Div>, items: &mut Vec<AnyElement>| {
            if !cells.is_empty() {
                items.push(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .gap(px(S3))
                        .children(cells.drain(..))
                        .into_any_element(),
                );
            }
        };
        let flush_card = |page: Div, items: &mut Vec<AnyElement>| {
            if items.is_empty() {
                return page;
            }
            page.child(card().gap(px(S3)).children(items.drain(..)))
        };

        for (i, widget) in view.iter().enumerate() {
            match widget {
                Widget::Section { title, about } => {
                    flush_cells(&mut cells, &mut card_items);
                    page = flush_card(page, &mut card_items);
                    page = page.child(section_rule(title, about.clone()));
                }
                Widget::Figure {
                    label,
                    value,
                    unit,
                    note,
                } => cells.push(figure_cell(label, value, unit, note.as_ref())),
                Widget::Meter {
                    label,
                    fraction,
                    note,
                } => cells.push(meter_cell(label, fraction, note.as_ref())),
                other => {
                    flush_cells(&mut cells, &mut card_items);
                    card_items.push(self.ext_widget(slot, i, other, cx));
                }
            }
        }
        flush_cells(&mut cells, &mut card_items);
        flush_card(page, &mut card_items)
    }

    fn ext_widget(
        &self,
        slot: ExtSlot,
        index: usize,
        widget: &Widget,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pal = p();
        let id = |what: &str| SharedString::from(format!("ext-{}-{what}-{index}", slot.0));
        match widget {
            Widget::Rows { items } => div()
                .flex()
                .flex_col()
                .children(items.iter().map(row_line))
                .into_any_element(),
            Widget::Switch {
                key,
                label,
                on,
                note,
            } => {
                let key = key.clone();
                let next = !*on;
                div()
                    .min_h(px(44.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(S3))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .font(body(FontWeight::MEDIUM))
                                    .text_size(px(T_BODY))
                                    .text_color(pal.text)
                                    .child(label.clone()),
                            )
                            .children(note_text(note.as_ref())),
                    )
                    .child(face_switch(id("switch"), *on).on_click(cx.listener(
                        move |this, _, _, cx| {
                            if let Some(host) = this.hub.ext.get() {
                                host.setting(slot, &key, next);
                            }
                            cx.notify();
                        },
                    )))
                    .into_any_element()
            }
            Widget::Button {
                action,
                label,
                kind,
            } => {
                let action = action.clone();
                let variant = match kind {
                    ButtonKind::Default => Variant::Default,
                    ButtonKind::Link => Variant::Link,
                };
                div()
                    .flex()
                    .flex_row()
                    .child(
                        button(id("button"), label.clone(), None, variant, true).on_click(
                            cx.listener(move |this, _, _, cx| {
                                if let Some(host) = this.hub.ext.get() {
                                    host.action(slot, &action);
                                }
                                cx.notify();
                            }),
                        ),
                    )
                    .into_any_element()
            }
            Widget::Note { text, kind } => match kind {
                NoteKind::Plain => div()
                    .font(body(FontWeight::NORMAL))
                    .text_size(px(T_BODY))
                    .line_height(px(20.0))
                    .text_color(pal.text_2)
                    .child(text.clone())
                    .into_any_element(),
                NoteKind::Error => error_line(text).into_any_element(),
            },
            // Laid out by `render_ext_widgets` itself.
            Widget::Section { .. } | Widget::Figure { .. } | Widget::Meter { .. } => {
                div().into_any_element()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use samo_primitives::UnsupportedReason;

    #[test]
    fn unknown_reading_draws_dashes() {
        let unknown = [
            Reading::unsupported(UnsupportedReason::NotReported),
            Reading::stopped(5),
            Reading::available(f64::NAN, 1),
        ];
        for value in &unknown {
            assert_eq!(figure_parts(value, "tokens").0, "--");
            assert_eq!(meter_share(value), None);
        }
        assert_eq!(
            figure_parts(&Reading::available(0.0, 1), "tokens"),
            ("0".into(), "tokens".into())
        );
        assert_eq!(
            figure_parts(&Reading::available(1_234_567.0, 1), "tokens"),
            ("1.23".into(), "M tokens".into())
        );
        assert_eq!(
            figure_parts(&Reading::available(12_345.0, 1), ""),
            ("12.3".into(), "k".into())
        );
        assert_eq!(meter_share(&Reading::available(1.7, 1)), Some(1.0));
    }

    #[test]
    fn only_error_lamp_is_signal() {
        for lamp in [Lamp::Working, Lamp::NeedsYou, Lamp::Done, Lamp::Off] {
            assert_ne!(lamp_tone(lamp), Tone::Signal, "{lamp:?}");
        }
        assert_eq!(lamp_tone(Lamp::Error), Tone::Signal);
    }
}
