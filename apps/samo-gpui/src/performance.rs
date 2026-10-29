//! Performance: the Mac's live detail under the readout — CPU history, each core, memory, disk and network.
//! A sensor that stopped leaves a gap, never a made-up line (R3).

use crate::art::{dot_number, led_column, led_meter};
use crate::charts::LineChart;
use crate::components::*;
use crate::overview::{cpu_series, pair, pressure_status};
use crate::shell::{Head, Page};
use crate::theme::*;
use crate::{Frame, SamoWorkspaceApp};
use gpui::{Div, FontWeight, IntoElement, ParentElement, Styled, div, prelude::*, px};
use samo_primitives::Reading;

/// History window shown in the CPU chart (samples arrive every ~1.2 s).
const CPU_WINDOW: usize = 120;

fn reading_bytes(r: &Reading<u64>) -> String {
    match r {
        Reading::Available { value, .. } => format_bytes(*value),
        Reading::Stopped { .. } => "stopped".to_string(),
        Reading::Unsupported { .. } => "—".to_string(),
    }
}

fn off(c: gpui::Hsla) -> gpui::Hsla {
    let mut c = c;
    c.a = 0.13;
    c
}

fn core_group(name: &str, prefix: &str, values: &[f32]) -> Div {
    let pal = p();
    div()
        .flex()
        .flex_col()
        .gap(px(S3))
        .child(mono_label(name, pal.text_2))
        .child(div().flex().flex_row().flex_wrap().gap(px(10.0)).children(
            values.iter().enumerate().map(|(i, pct)| {
                let pct = pct.clamp(0.0, 100.0);
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .font(mono(FontWeight::NORMAL))
                            .text_size(px(11.0))
                            .text_color(pal.text_2)
                            .child(format!("{pct:.0}")),
                    )
                    .child(led_column(
                        Some(pct / 100.0),
                        pal.accent,
                        pal.dot,
                        14.0,
                        88.0,
                    ))
                    .child(
                        div()
                            .font(mono(FontWeight::MEDIUM))
                            .text_size(px(11.0))
                            .text_color(pal.text)
                            .child(format!("{prefix}{}", i + 1)),
                    )
            }),
        ))
}

/// One I/O figure: its name, the rate in dots, and what it has added up to.
fn rate_cell(label: &str, rate: Option<u64>, total: Option<u64>) -> Div {
    let pal = p();
    let (fig, unit) = rate
        .map(bytes_parts)
        .unwrap_or_else(|| ("--".to_string(), "B"));
    div()
        .flex_1()
        .min_w(px(150.0))
        .p(px(S3))
        .rounded(px(R_CONTROL + 2.0))
        .bg(pal.shell)
        .border_1()
        .border_color(pal.hairline)
        .flex()
        .flex_col()
        .gap(px(S2))
        .child(mono_label(label, pal.text_2))
        .child(
            div()
                .flex()
                .flex_row()
                .items_end()
                .gap(px(6.0))
                .child(dot_number(
                    &fig,
                    20.0,
                    if rate.is_some() {
                        pal.accent
                    } else {
                        pal.text_2
                    },
                    off(pal.accent),
                ))
                .child(
                    div()
                        .font(mono(FontWeight::MEDIUM))
                        .text_size(px(12.0))
                        .text_color(pal.text_2)
                        .child(format!("{unit}/s")),
                ),
        )
        .child(
            div()
                .font(mono(FontWeight::NORMAL))
                .text_size(px(12.0))
                .text_color(pal.text_2)
                .child(
                    total
                        .map(|t| format!("{} total", format_bytes(t)))
                        .unwrap_or_else(|| "not available".to_string()),
                ),
        )
}

impl SamoWorkspaceApp {
    pub(crate) fn render_performance(&self, frame: &Frame) -> Page {
        let pal = p();
        let t = &frame.telemetry;
        let sys = &t.system;
        let sub = if frame.telemetry_ready {
            format!(
                "{} on {}, {} cores, up {}.",
                sys.os_version,
                sys.hostname,
                sys.logical_cores,
                format_uptime(sys.uptime_secs)
            )
        } else {
            "Samo is starting its sensors.".to_string()
        };

        let memory = match &t.latest.memory {
            Reading::Available { value, .. } => Some(value.clone()),
            _ => None,
        };
        let disk_io = match &t.latest.disk_io {
            Reading::Available { value, .. } => Some(value.clone()),
            _ => None,
        };
        let network = match &t.latest.network {
            Reading::Available { value, .. } => Some(value.clone()),
            _ => None,
        };
        let p_cores = match &t.latest.cpu_perf_cores_pct {
            Reading::Available { value, .. } => value.clone(),
            _ => Vec::new(),
        };
        let e_cores = match &t.latest.cpu_eff_cores_pct {
            Reading::Available { value, .. } => value.clone(),
            _ => Vec::new(),
        };

        let cpu_card = card()
            .child(card_head(
                "CPU load",
                None,
                Some(mono_label("last 2 minutes", pal.text_2).into_any_element()),
            ))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(S3))
                    .child(
                        div().flex_1().min_w(px(0.0)).h(px(150.0)).child(
                            LineChart::percent(
                                &cpu_series(frame),
                                CPU_WINDOW,
                                series_pair().0,
                                true,
                            )
                            .render(),
                        ),
                    )
                    .child(
                        div()
                            .h(px(150.0))
                            .flex()
                            .flex_col()
                            .justify_between()
                            .font(mono(FontWeight::NORMAL))
                            .text_size(px(11.0))
                            .text_color(pal.text_2)
                            .child("100")
                            .child("50")
                            .child("0"),
                    ),
            );

        let cores_card = card()
            .child(card_head(
                "Cores",
                None,
                Some(mono_label("load, %", pal.text_2).into_any_element()),
            ))
            .child(if p_cores.is_empty() && e_cores.is_empty() {
                loading("Waiting for this Mac to report load per core…")
            } else {
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap(px(S6))
                    .when(!p_cores.is_empty(), |el| {
                        el.child(core_group("Performance", "P", &p_cores))
                    })
                    .when(!e_cores.is_empty(), |el| {
                        el.child(core_group("Efficiency", "E", &e_cores))
                    })
            });

        let memory_card = card()
            .child(card_head(
                "Memory",
                None,
                memory.as_ref().map(|m| {
                    let (wrong, words) = pressure_status(m.pressure);
                    tag(
                        format!("pressure {words}"),
                        if wrong {
                            TagKind::Signal
                        } else {
                            TagKind::Neutral
                        },
                        true,
                    )
                    .into_any_element()
                }),
            ))
            .child(match memory {
                None => loading("Waiting for the first memory sample…"),
                Some(m) => {
                    let wrong = pressure_status(m.pressure).0;
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(S3))
                        .child(led_meter(
                            Some(ratio(m.used_bytes, m.total_bytes)),
                            if wrong { pal.signal } else { pal.accent },
                            pal.dot,
                            6.0,
                        ))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .child(kv_row(
                                    "Used",
                                    format!(
                                        "{} of {}",
                                        format_bytes(m.used_bytes),
                                        format_bytes(m.total_bytes)
                                    ),
                                ))
                                .child(kv_row("Available", format_bytes(m.available_bytes)))
                                .child(kv_row("Wired", reading_bytes(&m.wired_bytes)))
                                .child(kv_row("Compressed", reading_bytes(&m.compressed_bytes)))
                                .child(kv_row("Cached", reading_bytes(&m.cached_bytes)))
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
            });

        let iface = network.as_ref().map(|n| {
            if n.local_ip.is_empty() {
                n.primary_interface.clone()
            } else {
                format!("{} · {}", n.primary_interface, n.local_ip)
            }
        });
        let io_card = card()
            .child(card_head(
                "Disk & network",
                None,
                iface.map(|i| mono_label(&i, pal.text_2).into_any_element()),
            ))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap(px(S3))
                    .child(rate_cell(
                        "Disk read",
                        disk_io.as_ref().map(|d| d.read_bytes_per_sec),
                        disk_io.as_ref().map(|d| d.total_read_bytes),
                    ))
                    .child(rate_cell(
                        "Disk write",
                        disk_io.as_ref().map(|d| d.write_bytes_per_sec),
                        disk_io.as_ref().map(|d| d.total_write_bytes),
                    ))
                    .child(rate_cell(
                        "Network in",
                        network.as_ref().map(|n| n.rx_bytes_per_sec),
                        network.as_ref().map(|n| n.total_rx_bytes),
                    ))
                    .child(rate_cell(
                        "Network out",
                        network.as_ref().map(|n| n.tx_bytes_per_sec),
                        network.as_ref().map(|n| n.total_tx_bytes),
                    )),
            );

        Page {
            head: Head {
                icon: "chip",
                title: if frame.telemetry_ready {
                    sys.chip_brand.clone()
                } else {
                    "Performance".to_string()
                },
                sub,
                crumbs: false,
                actions: Vec::new(),
            },
            body: div()
                .flex()
                .flex_col()
                .gap(px(20.0))
                .child(cpu_card)
                .child(pair(
                    frame.layout.wide(),
                    cores_card,
                    memory_card,
                    Some(frame.layout.side_w() + 40.0),
                ))
                .child(io_card),
            fill: false,
        }
    }
}
