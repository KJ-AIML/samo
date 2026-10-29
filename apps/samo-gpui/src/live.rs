//! The live half of Overview: what this Mac is doing now and over the last hour, so the page is worth watching
//! while a folder is being measured. Everything comes from the sampler's history; a stopped sensor leaves a
//! gap, never a made-up line (R3).

use crate::art::led_meter;
use crate::charts::{ChartBounds, LineChart, Samples, slot_at, slot_x};
use crate::components::*;
use crate::hub::{Heat, SAMPLE_EVERY};
use crate::overview::{figure_cell, pressure_status};
use crate::theme::*;
use crate::{Frame, NavSection, ProcessSort, SamoWorkspaceApp};
use gpui::{
    Context, Div, FontWeight, Hsla, IntoElement, MouseMoveEvent, ParentElement, SharedString,
    Stateful, Styled, div, prelude::*, px, relative,
};
use samo_primitives::{Reading, TelemetryPoint};

/// Slots across every live chart: one a second over two minutes, wider buckets over longer spans.
const SLOTS: usize = 120;
/// How far the crosshair looks for a sample when the pointer sits between two.
const SNAP: usize = 3;
const TIP_W: f32 = 176.0;
/// The activity chart's least height; beside a long list it grows.
const CHART_MIN_H: f32 = 168.0;

/// How much history the live charts show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Span {
    TwoMin,
    TenMin,
    Hour,
}

impl Span {
    pub const ALL: [Self; 3] = [Self::TwoMin, Self::TenMin, Self::Hour];

    fn short(self) -> &'static str {
        match self {
            Self::TwoMin => "2m",
            Self::TenMin => "10m",
            Self::Hour => "1h",
        }
    }

    fn left_edge(self) -> &'static str {
        match self {
            Self::TwoMin => "2 min ago",
            Self::TenMin => "10 min ago",
            Self::Hour => "1 hour ago",
        }
    }

    /// Each slot's width in time. `SLOTS` of them make the span.
    fn bucket_ms(self) -> i64 {
        match self {
            Self::TwoMin => 1_000,
            Self::TenMin => 5_000,
            Self::Hour => 30_000,
        }
    }
}

/// The live charts that answer the pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveChart {
    Activity = 0,
    Network = 1,
    Disk = 2,
}

/// Where a chart is and which slot the pointer is over.
#[derive(Default)]
pub struct ChartHover {
    pub bounds: ChartBounds,
    pub slot: Option<usize>,
}

/// How a bucket of samples becomes one point: percentages average, rates keep their peak so a burst shows.
#[derive(Clone, Copy)]
enum Reduce {
    Mean,
    Peak,
}

/// Buckets `history` into the span's slots, newest at the right edge. A bucket whose samples all stopped is a
/// gap; a bucket no sample fell into is left out, so the line runs across it.
fn trace<const N: usize>(
    history: &[TelemetryPoint],
    span: Span,
    how: Reduce,
    read: impl Fn(&TelemetryPoint) -> [Option<f32>; N],
) -> [Samples; N] {
    let mut lines: [Samples; N] = std::array::from_fn(|_| Vec::new());
    let Some(newest) = history.last() else {
        return lines;
    };
    let b = span.bucket_ms();
    let last = newest.timestamp_ms.div_euclid(b);
    let first = last - (SLOTS as i64 - 1);
    let mut i = history.partition_point(|p| p.timestamp_ms.div_euclid(b) < first);
    while i < history.len() {
        let bucket = history[i].timestamp_ms.div_euclid(b);
        // (sum, peak, count) per line.
        let mut acc = [(0.0_f32, f32::MIN, 0_u32); N];
        while i < history.len() && history[i].timestamp_ms.div_euclid(b) == bucket {
            for (a, v) in acc.iter_mut().zip(read(&history[i])) {
                if let Some(v) = v {
                    a.0 += v;
                    a.1 = a.1.max(v);
                    a.2 += 1;
                }
            }
            i += 1;
        }
        let Ok(slot) = usize::try_from(bucket - first) else {
            continue;
        };
        if slot >= SLOTS {
            continue;
        }
        for (line, (sum, peak, n)) in lines.iter_mut().zip(acc) {
            let value = (n > 0).then(|| match how {
                Reduce::Mean => sum / n as f32,
                Reduce::Peak => peak,
            });
            line.push((slot, value));
        }
    }
    lines
}

fn available<T: Clone>(r: &Reading<T>) -> Option<T> {
    match r {
        Reading::Available { value, .. } => Some(value.clone()),
        _ => None,
    }
}

fn cpu_and_memory(p: &TelemetryPoint) -> [Option<f32>; 2] {
    let cpu = available(&p.cpu_overall_pct);
    let memory = match &p.memory {
        Reading::Available { value, .. } if value.total_bytes > 0 => {
            Some(value.used_bytes as f32 / value.total_bytes as f32 * 100.0)
        }
        _ => None,
    };
    [cpu, memory]
}

fn network(p: &TelemetryPoint) -> [Option<f32>; 2] {
    match &p.network {
        Reading::Available { value, .. } => [
            Some(value.rx_bytes_per_sec as f32),
            Some(value.tx_bytes_per_sec as f32),
        ],
        _ => [None, None],
    }
}

fn disk_io(p: &TelemetryPoint) -> [Option<f32>; 2] {
    match &p.disk_io {
        Reading::Available { value, .. } => [
            Some(value.read_bytes_per_sec as f32),
            Some(value.write_bytes_per_sec as f32),
        ],
        _ => [None, None],
    }
}

fn peak(lines: &[&Samples]) -> Option<f32> {
    lines
        .iter()
        .flat_map(|l| l.iter().filter_map(|s| s.1))
        .reduce(f32::max)
}

/// The top of a rate chart: the span's peak rounded up to 1, 2 or 5 of a unit, never under 100 KB/s, so an idle
/// line lies low instead of magnifying noise.
fn rate_scale(peak: f32) -> f32 {
    let peak = peak.max(64.0 * 1024.0);
    let mut unit = 1.0_f32;
    while peak / unit >= 1024.0 {
        unit *= 1024.0;
    }
    let m = peak / unit;
    let step = [1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1024.0]
        .into_iter()
        .find(|s| *s >= m)
        .unwrap_or(1024.0);
    step * unit
}

fn percent(v: f32) -> String {
    format!("{v:.1}%")
}

fn rate(v: f32) -> String {
    format_rate(v.max(0.0) as u64)
}

/// The slot nearest `slot` that the first line has a sample in.
fn snap(lines: &[(Samples, Hsla, &'static str)], slot: usize) -> Option<usize> {
    let first = &lines.first()?.0;
    first
        .iter()
        .map(|(s, _)| *s)
        .filter(|s| s.abs_diff(slot) <= SNAP)
        .min_by_key(|s| s.abs_diff(slot))
}

/// What Samo says about macOS's thermal state, and whether that is wrong (Signal).
fn heat_words(heat: Heat) -> (&'static str, &'static str, usize, bool) {
    match heat {
        Heat::Nominal => (
            "Nominal",
            "Running cool. Nothing is being held back.",
            1,
            false,
        ),
        Heat::Fair => (
            "Fair",
            "Warm. macOS may slow some background work.",
            2,
            false,
        ),
        Heat::Serious => (
            "Serious",
            "Hot. macOS is slowing the CPU to cool down.",
            3,
            true,
        ),
        Heat::Critical => (
            "Critical",
            "Very hot. macOS is slowing everything down.",
            4,
            true,
        ),
    }
}

/// Cards in rows of `cols`, equal widths and heights; a short last row keeps the widths. Each column is an
/// exact share of the row and carries half the gap on either side; its card fills the row's height.
fn grid(cols: usize, cards: Vec<Div>) -> Div {
    let cols = cols.max(1);
    let share = relative(1.0 / cols as f32);
    let mut rows = Vec::new();
    let mut it = cards.into_iter().peekable();
    while it.peek().is_some() {
        let row: Vec<Div> = it.by_ref().take(cols).collect();
        rows.push(
            div()
                .mx(px(-10.0))
                .flex()
                .flex_row()
                .children(row.into_iter().map(|card| {
                    div()
                        .w(share)
                        .flex_shrink_0()
                        .px(px(10.0))
                        .child(card.h_full())
                })),
        );
    }
    div().flex().flex_col().gap(px(20.0)).children(rows)
}

/// A card whose two halves sit at its top and bottom: beside a taller neighbour the room goes between them.
///
/// Nothing in a live card *grows* to take up room. Taffy, sizing a column with a growing child, makes it
/// taller than its content; a card that is only ever stretched from outside stays exactly its content.
fn ends(top: Div, bottom: Div) -> Div {
    card().justify_between().child(top).child(bottom)
}

fn stack() -> Div {
    div().flex().flex_col()
}

fn capitalised(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

fn legend_dot(color: Hsla) -> Div {
    div().size(px(8.0)).flex_shrink_0().rounded_full().bg(color)
}

fn small_mono(text: impl Into<SharedString>, color: Hsla) -> Div {
    div()
        .flex_shrink_0()
        .whitespace_nowrap()
        .font(mono(FontWeight::NORMAL))
        .text_size(px(T_LABEL))
        .text_color(color)
        .child(text.into())
}

impl SamoWorkspaceApp {
    /// The whole live section: activity beside the busiest processes, then the four gauges.
    pub(crate) fn render_live(&self, frame: &Frame, cx: &mut Context<Self>) -> Div {
        let l = frame.layout;
        let cols = if l.content_w >= 1060.0 {
            4
        } else if l.content_w >= 560.0 {
            2
        } else {
            1
        };
        div()
            .flex()
            .flex_col()
            .gap(px(20.0))
            .child(
                // Side by side the two cards share a height, and the chart takes what the list needs.
                div()
                    .flex()
                    .when(l.wide(), |el| el.flex_row())
                    .when(!l.wide(), |el| el.flex_col())
                    .gap(px(20.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .child(self.activity_card(frame, cx).h_full()),
                    )
                    .child(
                        div()
                            .when(l.wide(), |el| el.w(px(l.side_w())).flex_shrink_0())
                            .child(self.top_processes_card(frame, cx).h_full()),
                    ),
            )
            .child(grid(
                cols,
                vec![
                    self.network_card(frame, cx),
                    self.disk_io_card(frame, cx),
                    memory_card(frame),
                    thermal_card(frame),
                ],
            ))
    }

    fn span_switch(&self, cx: &mut Context<Self>) -> Div {
        segments(Span::ALL.into_iter().map(|span| {
            segment(
                ("live-span", span as usize),
                span.short(),
                span == self.live_span,
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.live_span = span;
                cx.notify();
            }))
        }))
    }

    /// A chart that answers the pointer with a crosshair and a note of every line at that moment.
    fn live_chart(
        &self,
        which: LiveChart,
        lines: Vec<(Samples, Hsla, &'static str)>,
        max: f32,
        show: fn(f32) -> String,
        height: Option<f32>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let pal = p();
        let hover = &self.live_hover[which as usize];
        let bounds = hover.bounds.clone();
        let mark = hover.slot.and_then(|s| snap(&lines, s));
        let width = bounds.get().map(|b| f32::from(b.size.width));

        let note = mark.zip(width).map(|(slot, w)| {
            let x = slot_x(slot, w, SLOTS);
            let secs = (SLOTS - 1 - slot) as i64 * self.live_span.bucket_ms() / 1000;
            let left = if x + 12.0 + TIP_W <= w {
                x + 12.0
            } else {
                (x - 12.0 - TIP_W).max(0.0)
            };
            div()
                .absolute()
                .top(px(4.0))
                .left(px(left))
                .w(px(TIP_W))
                .p(px(10.0))
                .flex()
                .flex_col()
                .gap(px(6.0))
                .rounded(px(R_CONTROL))
                .bg(pal.card)
                .border_1()
                .border_color(pal.hairline_strong)
                .shadow(shadow_pop())
                .child(small_mono(format_since(secs), pal.text_2))
                .children(lines.iter().map(|(samples, color, name)| {
                    let value = samples
                        .iter()
                        .find(|(s, _)| *s == slot)
                        .map(|s| s.1.map(show).unwrap_or_else(|| "stopped".to_string()))
                        .unwrap_or_else(|| "—".to_string());
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(S2))
                        .child(legend_dot(*color))
                        .child(
                            div()
                                .flex_1()
                                .font(body(FontWeight::NORMAL))
                                .text_size(px(T_META))
                                .text_color(pal.text_2)
                                .child(*name),
                        )
                        .child(
                            div()
                                .font(mono(FontWeight::MEDIUM))
                                .text_size(px(T_META))
                                .text_color(pal.text)
                                .child(value),
                        )
                }))
        });

        let chart = LineChart {
            lines: lines.into_iter().map(|(s, c, _)| (s, c)).collect(),
            capacity: SLOTS,
            max,
            gridlines: true,
            mark,
            bounds: Some(bounds),
        };
        div()
            .id(("live-chart", which as usize))
            .relative()
            .w_full()
            .map(|el| match height {
                Some(h) => el.h(px(h)),
                // Grow with the card, never below a readable height.
                None => el.h(px(CHART_MIN_H)).flex_grow(),
            })
            .child(chart.render())
            .children(note)
            .on_mouse_move(cx.listener(move |this, ev: &MouseMoveEvent, _, cx| {
                let h = &mut this.live_hover[which as usize];
                let slot = h.bounds.get().map(|b| {
                    slot_at(
                        f32::from(ev.position.x),
                        f32::from(b.origin.x),
                        f32::from(b.size.width),
                        SLOTS,
                    )
                });
                if slot != h.slot {
                    h.slot = slot;
                    cx.notify();
                }
            }))
            .on_hover(cx.listener(move |this, on: &bool, _, cx| {
                if !*on {
                    this.live_hover[which as usize].slot = None;
                    cx.notify();
                }
            }))
    }

    /// CPU and memory on one 0–100 % scale (both are a share of what this Mac has).
    fn activity_card(&self, frame: &Frame, cx: &mut Context<Self>) -> Div {
        let pal = p();
        let (c_cpu, c_mem) = series_pair();
        let span = self.live_span;
        let [cpu, mem] = trace(&frame.telemetry.history, span, Reduce::Mean, cpu_and_memory);
        let [cpu_now, mem_now] = cpu_and_memory(&frame.telemetry.latest);

        let legend = |name: &str, color: Hsla, now: Option<f32>, line: &Samples| {
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(S2))
                .child(legend_dot(color))
                .child(
                    div()
                        .font(body(FontWeight::MEDIUM))
                        .text_size(px(13.5))
                        .text_color(pal.text)
                        .child(name.to_string()),
                )
                .child(
                    div()
                        .font(mono(FontWeight::MEDIUM))
                        .text_size(px(13.5))
                        .text_color(pal.text)
                        .child(now.map(percent).unwrap_or_else(|| "--".to_string())),
                )
                .children(
                    peak(&[line]).map(|v| small_mono(format!("peak {}", percent(v)), pal.text_2)),
                )
        };

        card()
            .child(card_head(
                "Activity",
                None,
                Some(self.span_switch(cx).into_any_element()),
            ))
            .child(
                div()
                    .mb(px(S3))
                    .flex()
                    .flex_row()
                    .gap(px(S5))
                    .whitespace_nowrap()
                    .child(legend("CPU", c_cpu, cpu_now, &cpu))
                    .child(legend("Memory", c_mem, mem_now, &mem)),
            )
            .child(
                div()
                    .flex_grow()
                    .flex()
                    .flex_row()
                    .gap(px(S3))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .child(self.live_chart(
                                LiveChart::Activity,
                                vec![(cpu, c_cpu, "CPU"), (mem, c_mem, "Memory")],
                                100.0,
                                percent,
                                None,
                                cx,
                            ))
                            .child(
                                div()
                                    .h(px(14.0))
                                    .flex()
                                    .flex_row()
                                    .justify_between()
                                    .child(small_mono(span.left_edge(), pal.text_2))
                                    .child(small_mono("now", pal.text_2)),
                            ),
                    )
                    .child(
                        // The scale, beside the chart: its column stops where the time labels begin.
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .flex_grow()
                                    .flex()
                                    .flex_col()
                                    .justify_between()
                                    .items_end()
                                    .child(small_mono("100%", pal.text_2))
                                    .child(small_mono("50%", pal.text_2))
                                    .child(small_mono("0%", pal.text_2)),
                            )
                            .child(div().h(px(14.0))),
                    ),
            )
    }

    /// Two rates on one scale (in and out, read and write), with their figures now and the scale.
    #[allow(clippy::too_many_arguments)]
    fn rate_card(
        &self,
        which: LiveChart,
        title: &str,
        about: Option<String>,
        names: [&'static str; 2],
        now: Option<[u64; 2]>,
        lines: [Samples; 2],
        foot: String,
        cx: &mut Context<Self>,
    ) -> Div {
        let pal = p();
        let (c1, c2) = series_pair();
        let [first, second] = lines;
        let scale = rate_scale(peak(&[&first, &second]).unwrap_or(0.0));
        let figure = |name: &str, color: Hsla, value: Option<u64>| {
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(6.0))
                        .child(legend_dot(color))
                        .child(mono_label(name, pal.text_2)),
                )
                .child(
                    div()
                        .font(mono(FontWeight::MEDIUM))
                        .text_size(px(16.0))
                        .text_color(if value.is_some() {
                            pal.text
                        } else {
                            pal.text_2
                        })
                        .whitespace_nowrap()
                        .child(value.map(format_rate).unwrap_or_else(|| "--".to_string())),
                )
        };
        let top = stack()
            .child(card_head(
                title.to_string(),
                None,
                about.map(|a| small_mono(a, pal.text_2).into_any_element()),
            ))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(S3))
                    .child(figure(names[0], c1, now.map(|n| n[0])))
                    .child(figure(names[1], c2, now.map(|n| n[1]))),
            );
        let bottom = stack()
            .pt(px(S3))
            .child(self.live_chart(
                which,
                vec![(first, c1, names[0]), (second, c2, names[1])],
                scale,
                rate,
                Some(64.0),
                cx,
            ))
            .child(
                div()
                    .mt(px(S2))
                    .flex()
                    .flex_row()
                    .gap(px(S2))
                    .child(mono_line(foot, T_LABEL, pal.text_2).flex_1())
                    .child(small_mono(
                        format!("scale {}", format_rate(scale as u64)),
                        pal.text_2,
                    )),
            );
        ends(top, bottom)
    }

    fn network_card(&self, frame: &Frame, cx: &mut Context<Self>) -> Div {
        let t = &frame.telemetry;
        let net = available(&t.latest.network);
        let lines = trace(&t.history, self.live_span, Reduce::Peak, network);
        let about = net.as_ref().map(|n| n.primary_interface.clone());
        let foot = net
            .as_ref()
            .map(|n| {
                let ip = if n.local_ip.is_empty() {
                    String::new()
                } else {
                    format!("{} · ", n.local_ip)
                };
                format!(
                    "{ip}{} in, {} out",
                    format_bytes(n.total_rx_bytes),
                    format_bytes(n.total_tx_bytes)
                )
            })
            .unwrap_or_else(|| "Waiting for the first sample".to_string());
        self.rate_card(
            LiveChart::Network,
            "Network",
            about,
            ["In", "Out"],
            net.map(|n| [n.rx_bytes_per_sec, n.tx_bytes_per_sec]),
            lines,
            foot,
            cx,
        )
    }

    fn disk_io_card(&self, frame: &Frame, cx: &mut Context<Self>) -> Div {
        let t = &frame.telemetry;
        let io = available(&t.latest.disk_io);
        let lines = trace(&t.history, self.live_span, Reduce::Peak, disk_io);
        let foot = io
            .as_ref()
            .map(|d| {
                format!(
                    "{} read, {} written",
                    format_bytes(d.total_read_bytes),
                    format_bytes(d.total_write_bytes)
                )
            })
            .unwrap_or_else(|| "Waiting for the first sample".to_string());
        self.rate_card(
            LiveChart::Disk,
            "Disk activity",
            None,
            ["Read", "Write"],
            io.map(|d| [d.read_bytes_per_sec, d.write_bytes_per_sec]),
            lines,
            foot,
            cx,
        )
    }

    /// The busiest processes, by CPU or by memory (the same choice as the Processes page).
    fn top_processes_card(&self, frame: &Frame, cx: &mut Context<Self>) -> Div {
        let pal = p();
        let by_cpu = self.proc_sort == ProcessSort::Cpu;
        let sort_switch = segments(
            [(ProcessSort::Cpu, "CPU"), (ProcessSort::Memory, "Memory")].map(|(sort, label)| {
                segment(("ov-sort", sort as usize), label, self.proc_sort == sort).on_click(
                    cx.listener(move |this, _, _, cx| {
                        this.proc_sort = sort;
                        this.refresh_processes(cx);
                    }),
                )
            }),
        );
        let body: Div = if frame.telemetry.processes.is_empty() {
            loading("Reading the process list…")
        } else {
            div()
                .flex()
                .flex_col()
                .children(frame.telemetry.processes.iter().take(7).map(|proc_row| {
                    div()
                        .h(px(36.0))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(S3))
                        .border_b_1()
                        .border_color(pal.hairline)
                        .child(
                            line(proc_row.name.clone(), FontWeight::MEDIUM, 13.5, pal.text)
                                .flex_1(),
                        )
                        .child(figure_cell(
                            format!("{:.1}%", proc_row.cpu_pct),
                            54.0,
                            by_cpu,
                        ))
                        .child(figure_cell(
                            format_bytes(proc_row.memory_bytes),
                            76.0,
                            !by_cpu,
                        ))
                }))
        };
        ends(
            stack()
                .child(card_head(
                    "Top processes",
                    None,
                    Some(sort_switch.into_any_element()),
                ))
                .child(body),
            div().pt(px(S2)).flex().flex_row().justify_end().child(
                button(
                    "ov-all-procs",
                    format!(
                        "All {} processes",
                        format_count(frame.telemetry.total_processes as u64)
                    ),
                    None,
                    Variant::Link,
                    true,
                )
                .on_click(cx.listener(|this, _, _, cx| this.set_nav(NavSection::Processes, cx))),
            ),
        )
    }
}

fn memory_card(frame: &Frame) -> Div {
    let pal = p();
    let (_, c_mem) = series_pair();
    let memory = available(&frame.telemetry.latest.memory);
    let head = card_head(
        "Memory",
        None,
        memory.as_ref().map(|m| {
            let (wrong, words) = pressure_status(m.pressure);
            // The state alone, as Thermal's is: "pressure elevated" overruns a quarter-width card.
            tag(
                capitalised(words),
                if wrong {
                    TagKind::Signal
                } else {
                    TagKind::Neutral
                },
                true,
            )
            .into_any_element()
        }),
    );
    let Some(m) = memory else {
        return card()
            .child(head)
            .child(loading("Waiting for the first memory sample…"));
    };
    let wrong = pressure_status(m.pressure).0;
    let read = |r: &Reading<u64>| available(r).map(format_bytes).unwrap_or_else(|| "—".into());
    let top = stack()
        .child(head)
        .child(
            div()
                .flex()
                .flex_row()
                .items_end()
                .gap(px(6.0))
                .whitespace_nowrap()
                .child(
                    div()
                        .font(mono(FontWeight::MEDIUM))
                        .text_size(px(16.0))
                        .text_color(pal.text)
                        .child(format_bytes(m.used_bytes)),
                )
                .child(
                    div()
                        .font(mono(FontWeight::NORMAL))
                        .text_size(px(T_META))
                        .text_color(pal.text_2)
                        .child(format!("used of {}", format_bytes(m.total_bytes))),
                ),
        )
        .child(div().mt(px(S3)).child(led_meter(
            Some(ratio(m.used_bytes, m.total_bytes)),
            if wrong { pal.signal } else { c_mem },
            pal.dot,
            6.0,
        )));
    ends(
        top,
        stack()
            .pt(px(S3))
            .child(kv_row("Compressed", read(&m.compressed_bytes)))
            .child(kv_row(
                "Swap",
                format!(
                    "{} of {}",
                    format_bytes(m.swap_used_bytes),
                    format_bytes(m.swap_total_bytes)
                ),
            )),
    )
}

/// macOS's thermal state as a four-step meter. Only Serious and Critical are wrong.
fn thermal_card(frame: &Frame) -> Div {
    let pal = p();
    let (c_first, _) = series_pair();
    let Some(thermal) = frame.thermal else {
        return card()
            .child(card_head("Thermal", None, None))
            .child(loading("Asking macOS how warm this Mac is…"));
    };
    let (word, sentence, lit, wrong) = heat_words(thermal.heat);
    let on = if wrong { pal.signal } else { c_first };
    let top = stack()
        .child(card_head(
            "Thermal",
            None,
            Some(
                tag(
                    word,
                    if wrong {
                        TagKind::Signal
                    } else {
                        TagKind::Neutral
                    },
                    true,
                )
                .into_any_element(),
            ),
        ))
        .child(
            div()
                .flex()
                .flex_row()
                .gap(px(4.0))
                .children((0..4).map(|i| {
                    div()
                        .flex_1()
                        .h(px(6.0))
                        .bg(if i < lit { on } else { pal.dot })
                })),
        )
        .child(
            // Two lines, always: a box whose height doesn't depend on how its words wrap.
            div()
                .mt(px(S3))
                .h(px(36.0))
                .font(body(FontWeight::NORMAL))
                .text_size(px(T_META))
                .line_height(px(18.0))
                .text_color(pal.text_2)
                .child(sentence),
        );
    ends(
        top,
        stack()
            .pt(px(S3))
            .child(kv_row(
                "Low Power Mode",
                if thermal.low_power { "on" } else { "off" }.to_string(),
            ))
            .child(kv_row(
                "Up",
                if frame.telemetry_ready {
                    format_uptime(frame.telemetry.system.uptime_secs)
                } else {
                    "—".to_string()
                },
            )),
    )
}

/// The sampler's cadence in words, for the section's rule.
pub fn cadence_words() -> String {
    format!("sampled every {:.1} s", SAMPLE_EVERY.as_secs_f32())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(ts: i64, cpu: Option<f32>) -> TelemetryPoint {
        let mut p = crate::hub::placeholder_telemetry().latest;
        p.timestamp_ms = ts;
        p.cpu_overall_pct = match cpu {
            Some(v) => Reading::available(v, ts),
            None => Reading::stopped(ts),
        };
        p
    }

    fn cpu(p: &TelemetryPoint) -> [Option<f32>; 1] {
        [available(&p.cpu_overall_pct)]
    }

    #[test]
    fn samples_land_in_time_slots_with_the_newest_at_the_right_edge() {
        let history: Vec<_> = (0..6).map(|i| point(i * 1_200, Some(i as f32))).collect();
        let [line] = trace(&history, Span::TwoMin, Reduce::Mean, cpu);
        let slots: Vec<usize> = line.iter().map(|s| s.0).collect();
        // 0, 1.2, 2.4, 3.6, 4.8, 6.0 s: second 5 has no sample, so it is skipped over, not a gap.
        assert_eq!(slots, vec![113, 114, 115, 116, 117, 119]);
        assert!(line.iter().all(|s| s.1.is_some()));
    }

    #[test]
    fn a_stopped_sensor_is_a_gap_and_old_samples_fall_off() {
        let mut history = vec![point(0, Some(50.0))];
        history.push(point(200_000, None));
        history.push(point(201_000, Some(10.0)));
        let [line] = trace(&history, Span::TwoMin, Reduce::Mean, cpu);
        assert_eq!(line, vec![(118, None), (119, Some(10.0))]);
    }

    #[test]
    fn buckets_average_percentages_and_keep_the_peak_of_rates() {
        let history: Vec<_> = [10.0, 30.0, 20.0]
            .into_iter()
            .enumerate()
            .map(|(i, v)| point(60_000 + i as i64 * 1_000, Some(v)))
            .collect();
        let [mean] = trace(&history, Span::TenMin, Reduce::Mean, cpu);
        let [top] = trace(&history, Span::TenMin, Reduce::Peak, cpu);
        assert_eq!(mean, vec![(119, Some(20.0))]);
        assert_eq!(top, vec![(119, Some(30.0))]);
    }

    #[test]
    fn rate_scales_round_up_and_never_magnify_an_idle_line() {
        const KB: f32 = 1024.0;
        assert_eq!(rate_scale(0.0), 100.0 * KB);
        assert_eq!(rate_scale(300.0 * KB), 500.0 * KB);
        assert_eq!(rate_scale(1.3 * KB * KB), 2.0 * KB * KB);
        assert_eq!(rate_scale(700.0 * KB), KB * KB);
    }

    #[test]
    fn short_times_read_naturally() {
        assert_eq!(format_since(0), "now");
        assert_eq!(format_since(34), "34 s ago");
        assert_eq!(format_since(720), "12 min ago");
    }
}
