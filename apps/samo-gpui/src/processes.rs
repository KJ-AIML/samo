//! Processes: a live table. Only the rows on screen are built (a virtualised list under a sticky header),
//! columns step away as the window narrows, and while the pointer is over the table its rows hold still,
//! so a row can't move under a click. Quitting is the one irreversible act here, so it reads in Signal, and
//! Samo re-checks the process's identity (PID + start time) before sending it.

use crate::components::*;
use crate::shell::{Head, Page};
use crate::theme::*;
use crate::{Frame, ProcessSort, SamoWorkspaceApp};
use gpui::{
    AnyElement, Context, Div, FontWeight, IntoElement, ParentElement, SharedString, Stateful,
    Styled, div, prelude::*, px, uniform_list,
};
use samo_primitives::EnergyImpact;
use std::ops::Range;
use std::sync::Arc;

const W_PID: f32 = 64.0;
const W_CPU: f32 = 64.0;
const W_MEM: f32 = 88.0;
const W_DISK: f32 = 92.0;
const W_ENERGY: f32 = 100.0;
const W_ACTION: f32 = 68.0;
const ROW_H: f32 = 40.0;

/// Which columns fit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Columns {
    pub pid: bool,
    pub disk: bool,
    pub energy: bool,
}

impl Columns {
    pub fn all() -> Self {
        Self {
            pid: true,
            disk: true,
            energy: true,
        }
    }

    pub fn for_width(table_w: f32) -> Self {
        Self {
            pid: table_w >= 560.0,
            disk: table_w >= 660.0,
            energy: table_w >= 780.0,
        }
    }
}

fn energy_tag(e: EnergyImpact) -> Div {
    match e {
        EnergyImpact::Low => tag("low", TagKind::Neutral, false),
        EnergyImpact::Moderate => tag("moderate", TagKind::Neutral, false),
        EnergyImpact::High => tag("high", TagKind::Neutral, true),
        EnergyImpact::VeryHigh => tag("very high", TagKind::Neutral, true),
    }
}

fn cell_right(width: f32) -> Div {
    div()
        .w(px(width))
        .flex_shrink_0()
        .flex()
        .flex_row()
        .items_center()
        .justify_end()
}

impl SamoWorkspaceApp {
    pub(crate) fn render_processes(&mut self, frame: &Frame, cx: &mut Context<Self>) -> Page {
        let pal = p();
        let live = frame.telemetry.clone();
        let live_is_current = frame.telemetry_view == (self.proc_sort, self.include_self);

        // Hold the rows while the pointer is over the table; let go when it leaves.
        if self.table_hovered {
            if self.held_procs.is_none() && live_is_current {
                self.held_procs = Some(live.clone());
            }
        } else {
            self.held_procs = None;
        }
        let rows = self.held_procs.clone().unwrap_or_else(|| live.clone());
        let holding = !Arc::ptr_eq(&rows, &live);
        let cols = Columns::for_width(frame.layout.content_w - 2.0 * S2);
        self.proc_rows = rows.clone();
        self.proc_cols = cols;

        let sub = format!(
            "{} running; the top {} by {}.",
            format_count(live.total_processes as u64),
            live.processes.len(),
            self.proc_sort.label()
        );

        let include_self = self.include_self;
        let toggle = div()
            .id("self-toggle")
            .flex()
            .flex_row()
            .items_center()
            .gap(px(10.0))
            .cursor_pointer()
            .on_click(cx.listener(|this, _, _, cx| {
                this.include_self = !this.include_self;
                this.refresh_processes(cx);
            }))
            .child(face_switch("self-switch", include_self))
            .child(
                div()
                    .font(body(FontWeight::MEDIUM))
                    .text_size(px(T_BODY))
                    .text_color(pal.text)
                    .child("Include Samo"),
            );

        let header = div()
            .h(px(36.0))
            .px(px(S3))
            .flex()
            .flex_shrink_0()
            .flex_row()
            .items_center()
            .gap(px(S2))
            .border_b_1()
            .border_color(pal.hairline)
            .font(body(FontWeight::MEDIUM))
            .text_size(px(T_META))
            .text_color(pal.text_2)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(S2))
                    .child("Name")
                    .when(holding, |el| {
                        el.child(tag("held while you point", TagKind::Accent, true))
                    }),
            )
            .when(cols.pid, |el| {
                el.child(self.sort_header("PID", ProcessSort::Pid, W_PID, cx))
            })
            .child(self.sort_header("CPU", ProcessSort::Cpu, W_CPU, cx))
            .child(self.sort_header("Memory", ProcessSort::Memory, W_MEM, cx))
            .when(cols.disk, |el| {
                el.child(self.sort_header("Disk I/O", ProcessSort::Disk, W_DISK, cx))
            })
            .when(cols.energy, |el| {
                el.child(cell_right(W_ENERGY).child("Energy"))
            })
            .child(div().w(px(W_ACTION)).flex_shrink_0());

        let count = rows.processes.len();
        let list: AnyElement = if count == 0 {
            div()
                .p(px(S3))
                .child(loading("Reading the process list…"))
                .into_any_element()
        } else {
            uniform_list(
                "process-rows",
                count,
                cx.processor(|this, range: Range<usize>, _window, cx| this.process_rows(range, cx)),
            )
            .track_scroll(self.proc_scroll.clone())
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .into_any_element()
        };

        Page {
            head: Head {
                icon: "pulse",
                title: "Processes".to_string(),
                sub,
                crumbs: false,
                actions: vec![toggle.into_any_element()],
            },
            body: div().flex().flex_col().child(
                card()
                    .id("process-table")
                    .flex_1()
                    .min_h(px(0.0))
                    .px(px(S2))
                    .py(px(S2))
                    .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                        this.table_hovered = *hovered;
                        if !*hovered {
                            this.held_procs = None;
                        }
                        cx.notify();
                    }))
                    .child(header)
                    .child(list),
            ),
            fill: true,
        }
    }

    /// The rows on screen, and only those.
    fn process_rows(&mut self, range: Range<usize>, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let pal = p();
        let rows = self.proc_rows.clone();
        let cols = self.proc_cols;
        let sorted = self.proc_sort;
        range
            .filter_map(|i| rows.processes.get(i).map(|r| (i, r)))
            .map(|(i, proc_row)| {
                let identity = proc_row.identity;
                let name = proc_row.name.clone();
                let disk_rate =
                    proc_row.disk_read_bytes_per_sec + proc_row.disk_write_bytes_per_sec;
                let path_tip = SharedString::from(if proc_row.executable_path.is_empty() {
                    proc_row.name.clone()
                } else {
                    proc_row.executable_path.clone()
                });
                let figure = |text: String, width: f32, strong: bool| {
                    cell_right(width)
                        .font(mono(if strong {
                            FontWeight::MEDIUM
                        } else {
                            FontWeight::NORMAL
                        }))
                        .text_size(px(T_META))
                        .text_color(if strong { pal.text } else { pal.text_2 })
                        .child(text)
                };

                div()
                    .id(SharedString::from(format!("proc-{}", identity.pid)))
                    .group("proc-row")
                    .w_full()
                    .h(px(ROW_H))
                    .px(px(S3))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(S2))
                    .border_b_1()
                    .border_color(pal.hairline)
                    .hover(|s| s.bg(pal.hover))
                    .child(
                        div()
                            .id(SharedString::from(format!("proc-name-{i}")))
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(S2))
                            .tooltip(move |_, cx| tip(path_tip.clone(), cx))
                            .child(
                                line(proc_row.name.clone(), FontWeight::MEDIUM, T_BODY, pal.text)
                                    .flex_1(),
                            )
                            .when(proc_row.is_self, |el| {
                                el.child(tag("Samo", TagKind::Accent, false))
                            })
                            // a bare uid says nothing; a named user (root, _windowserver) does
                            .when(!proc_row.user.chars().all(|c| c.is_ascii_digit()), |el| {
                                el.child(
                                    div()
                                        .flex_shrink_0()
                                        .font(mono(FontWeight::NORMAL))
                                        .text_size(px(12.0))
                                        .text_color(pal.text_2)
                                        .child(proc_row.user.clone()),
                                )
                            }),
                    )
                    .when(cols.pid, |el| {
                        el.child(figure(
                            identity.pid.to_string(),
                            W_PID,
                            sorted == ProcessSort::Pid,
                        ))
                    })
                    .child(figure(
                        format!("{:.1}%", proc_row.cpu_pct),
                        W_CPU,
                        sorted == ProcessSort::Cpu,
                    ))
                    .child(figure(
                        format_bytes(proc_row.memory_bytes),
                        W_MEM,
                        sorted == ProcessSort::Memory,
                    ))
                    .when(cols.disk, |el| {
                        el.child(figure(
                            if disk_rate == 0 {
                                "—".to_string()
                            } else {
                                format_rate(disk_rate)
                            },
                            W_DISK,
                            sorted == ProcessSort::Disk,
                        ))
                    })
                    .when(cols.energy, |el| {
                        el.child(cell_right(W_ENERGY).child(energy_tag(proc_row.energy_impact)))
                    })
                    .child(
                        cell_right(W_ACTION)
                            .opacity(0.0)
                            .group_hover("proc-row", |s| s.opacity(1.0))
                            .child(
                                button(
                                    SharedString::from(format!("proc-quit-{i}")),
                                    "Quit",
                                    None,
                                    Variant::Danger,
                                    true,
                                )
                                .tooltip(|_, cx| {
                                    tip(
                                        "Send SIGTERM, after checking it is still the same process"
                                            .into(),
                                        cx,
                                    )
                                })
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.quit_process(identity, name.clone(), "SIGTERM", cx)
                                    },
                                )),
                            ),
                    )
                    .into_any_element()
            })
            .collect()
    }

    fn sort_header(
        &self,
        label: &'static str,
        sort: ProcessSort,
        width: f32,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let pal = p();
        let current = self.proc_sort == sort;
        cell_right(width)
            .id(SharedString::from(format!("sort-{}", sort.as_str())))
            .gap(px(S1))
            .cursor_pointer()
            .text_color(if current { pal.text } else { pal.text_2 })
            .hover(|s| s.text_color(pal.text))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.proc_sort = sort;
                this.refresh_processes(cx);
            }))
            .when(current, |el| {
                el.child(icon("chevron-down", 12.0, pal.accent_text))
            })
            .child(label)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_step_away_as_the_table_narrows() {
        assert_eq!(Columns::for_width(1000.0), Columns::all());
        let mid = Columns::for_width(700.0);
        assert!(mid.pid && mid.disk && !mid.energy);
        let small = Columns::for_width(500.0);
        assert!(!small.pid && !small.disk && !small.energy);
    }
}
