//! Processes: a live table of apps. Helpers are rolled up into the app they belong to ("Grok Bot", 11 processes; on
//! Windows, every process of one .exe), each app has its icon, apps in the Dock (Windows: on the taskbar) are set
//! apart from background processes, and a search box narrows it all. Only the rows on screen are built (a
//! virtualised list under a sticky header), columns step
//! away as the window narrows, and while the pointer is over the table its rows hold still, so a row can't move
//! under a click. Quitting is the one irreversible act here, so it reads in Signal, is offered for one process at a
//! time (never for a whole app), and Samo re-checks the process's identity (PID + start time) before sending it.

use crate::app_icons::{self, AppIcons};
use crate::components::*;
use crate::shell::{Head, Page};
use crate::text_input::{Changed, TextInput};
use crate::theme::*;
use crate::{Frame, ProcessSort, SamoWorkspaceApp};
use gpui::{
    AnyElement, Context, Div, Entity, FontWeight, IntoElement, ObjectFit, ParentElement,
    SharedString, Stateful, Styled, Subscription, div, img, prelude::*, px, uniform_list,
};
use samo_domain::{GroupKey, ProcessGroup, group_processes_on, sort_groups, visible_members};
use samo_primitives::{EnergyImpact, ProcessIdentity};
use std::collections::HashSet;
use std::ops::Range;
use std::sync::Arc;

const W_PID: f32 = 64.0;
const W_CPU: f32 = 76.0;
const W_MEM: f32 = 100.0;
const W_DISK: f32 = 92.0;
const W_ENERGY: f32 = 100.0;
const W_ACTION: f32 = 68.0;
const ROW_H: f32 = 40.0;
/// An app's icon in a row.
const ICON_PX: f32 = 24.0;
/// The least a CPU bar's full width means (a core-percentage) and a memory bar's (bytes): a quiet Mac shows short
/// bars, not full ones for 0.1%.
const CPU_FLOOR: f32 = 50.0;
const MEM_FLOOR: u64 = 1 << 30;

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

/// One row of the virtualised list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    /// "Applications (6)" or "Background (662)".
    Heading { application: bool, count: usize },
    /// An app with all its processes (a chevron opens it), or a process on its own. `group` indexes the groups.
    Group { group: usize, open: bool },
    /// One process of an open app. `row` indexes the snapshot's processes.
    Member { group: usize, row: usize },
}

/// Lays the list out: applications (the Dock's), then background, each with its heading, each app followed by its
/// processes while it is open. `visible[g]` is what typing in the search leaves of group `g` (empty: the group is
/// hidden). Typing opens every app, so a match inside one is seen. Where the OS hasn't said which are applications
/// (not yet, or not on this OS) there are no headings and one list.
pub fn layout_lines(
    groups: &[ProcessGroup],
    visible: &[Vec<usize>],
    expanded: &HashSet<GroupKey>,
    searching: bool,
    apps_known: bool,
) -> Vec<Line> {
    let mut lines = Vec::new();
    let sections: &[bool] = if apps_known { &[true, false] } else { &[false] };
    for &application in sections {
        let mine: Vec<usize> = (0..groups.len())
            .filter(|&g| {
                visible.get(g).is_some_and(|v| !v.is_empty())
                    && (!apps_known || groups[g].application == application)
            })
            .collect();
        if apps_known {
            if mine.is_empty() {
                continue;
            }
            lines.push(Line::Heading {
                application,
                count: mine.len(),
            });
        }
        for g in mine {
            let several = groups[g].members.len() > 1;
            let open = several && (searching || expanded.contains(&groups[g].key));
            lines.push(Line::Group { group: g, open });
            if open {
                lines.extend(visible[g].iter().map(|&row| Line::Member { group: g, row }));
            }
        }
    }
    lines
}

/// The Processes page's own state in the window: the search, which apps are open, and what the virtualised list
/// draws from (set each render).
pub struct ProcUi {
    pub search: Entity<TextInput>,
    pub expanded: HashSet<GroupKey>,
    lines: Vec<Line>,
    groups: Vec<ProcessGroup>,
    /// What a bar's full width is: the biggest CPU and the biggest memory on screen, never less than a floor so an
    /// idle Mac isn't a page of full bars.
    scale: (f32, u64),
    _search_changed: Subscription,
}

impl ProcUi {
    pub fn new(cx: &mut Context<SamoWorkspaceApp>) -> Self {
        let search = cx.new(|cx| TextInput::new("Filter by name, path or PID", cx));
        let sub = cx.subscribe(&search, |_, _, _: &Changed, cx| cx.notify());
        Self {
            search,
            expanded: HashSet::new(),
            lines: Vec::new(),
            groups: Vec::new(),
            scale: (CPU_FLOOR, MEM_FLOOR),
            _search_changed: sub,
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

/// An app's icon, `size` points square, for a row. Its app is folder `bundle` (the Mac) or that .exe (Windows); a
/// process in no app, and an app the OS has no icon for, gets a gear. While an icon loads its place is empty, so
/// rows don't shift when it arrives. An OS that can't tell draws none (`None`): said, never faked.
pub(crate) fn app_icon(icons: &AppIcons, bundle: Option<&str>, size: f32) -> Option<AnyElement> {
    if !app_icons::ON_THIS_OS {
        return None;
    }
    let pal = p();
    let slot = div()
        .size(px(size))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center();
    Some(match bundle {
        Some(bundle) => match icons.icon(bundle) {
            Some(image) => slot
                .child(img(image).size(px(size)).object_fit(ObjectFit::Contain))
                .into_any_element(),
            None if icons.is_missing(bundle) => slot
                .child(icon("gear", size * 0.66, pal.text_2))
                .into_any_element(),
            None => slot.into_any_element(),
        },
        None => slot
            .child(icon("gear", size * 0.66, pal.text_2))
            .into_any_element(),
    })
}

/// A figure with a thin bar under it: how big this is next to the biggest on screen. A bar under 2 px would be a
/// smudge, so a value that is more than nothing gets at least that.
fn bar_figure(text: String, width: f32, frac: f32, strong: bool) -> Div {
    let pal = p();
    let bar = if frac > 0.002 {
        ((width - 6.0) * frac.clamp(0.0, 1.0)).max(2.0)
    } else {
        0.0
    };
    let mut line = pal.accent;
    line.a = 0.7;
    cell_right(width)
        .relative()
        .h(px(30.0))
        .child(
            div()
                .absolute()
                .left_0()
                .bottom(px(1.0))
                .h(px(3.0))
                .w(px(bar))
                .rounded(px(1.5))
                .bg(line),
        )
        .child(
            div()
                .relative()
                .pr(px(6.0))
                .pb(px(3.0))
                .font(mono(if strong {
                    FontWeight::MEDIUM
                } else {
                    FontWeight::NORMAL
                }))
                .text_size(px(T_META))
                .text_color(if strong { pal.text } else { pal.text_2 })
                .child(text),
        )
}

/// What one row says; `line_row` draws it.
struct RowSpec {
    id: SharedString,
    /// A chevron slot (open or shut; `None` for a row with nothing to open), then the icon.
    chevron: Option<bool>,
    indent: bool,
    icon: Option<AnyElement>,
    name: String,
    /// Quietly after the name: "11 processes".
    detail: Option<String>,
    tip: SharedString,
    is_self: bool,
    /// A named user (root), not a bare uid, which says nothing.
    user: Option<String>,
    pid: u32,
    cpu_pct: f32,
    memory_bytes: u64,
    disk_rate: u64,
    energy: EnergyImpact,
    /// Present only where Quit is offered: one process, with the identity to re-check.
    quit: Option<(ProcessIdentity, String)>,
}

impl SamoWorkspaceApp {
    fn line_row(
        &self,
        spec: RowSpec,
        sorted: ProcessSort,
        scale: (f32, u64),
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let pal = p();
        let cols = self.proc_cols;
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
        let tip_text = spec.tip.clone();
        let chevron: AnyElement = match spec.chevron {
            Some(open) => icon(
                if open {
                    "chevron-down"
                } else {
                    "chevron-right"
                },
                14.0,
                pal.text_2,
            )
            .into_any_element(),
            None => div().size(px(14.0)).into_any_element(),
        };
        let cpu_frac = ratio_f(spec.cpu_pct, scale.0);
        let mem_frac = ratio(spec.memory_bytes, scale.1);
        div()
            .id(spec.id.clone())
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
                    .id(SharedString::from(format!("{}-name", spec.id)))
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(S2))
                    .when(spec.indent, |el| el.pl(px(S5)))
                    .tooltip(move |_, cx| tip(tip_text.clone(), cx))
                    .child(chevron)
                    .children(spec.icon)
                    .child(line(spec.name, FontWeight::MEDIUM, T_BODY, pal.text).flex_1())
                    .children(spec.detail.map(|d| {
                        div()
                            .flex_shrink_0()
                            .font(mono(FontWeight::NORMAL))
                            .text_size(px(12.0))
                            .text_color(pal.text_2)
                            .child(d)
                    }))
                    .when(spec.is_self, |el| {
                        el.child(tag("Samo", TagKind::Accent, false))
                    })
                    .children(spec.user.map(|u| {
                        div()
                            .flex_shrink_0()
                            .font(mono(FontWeight::NORMAL))
                            .text_size(px(12.0))
                            .text_color(pal.text_2)
                            .child(u)
                    })),
            )
            .when(cols.pid, |el| {
                el.child(figure(
                    spec.pid.to_string(),
                    W_PID,
                    sorted == ProcessSort::Pid,
                ))
            })
            .child(bar_figure(
                format!("{:.1}%", spec.cpu_pct),
                W_CPU,
                cpu_frac,
                sorted == ProcessSort::Cpu,
            ))
            .child(bar_figure(
                format_bytes(spec.memory_bytes),
                W_MEM,
                mem_frac,
                sorted == ProcessSort::Memory,
            ))
            .when(cols.disk, |el| {
                el.child(figure(
                    if spec.disk_rate == 0 {
                        "—".to_string()
                    } else {
                        format_rate(spec.disk_rate)
                    },
                    W_DISK,
                    sorted == ProcessSort::Disk,
                ))
            })
            .when(cols.energy, |el| {
                el.child(cell_right(W_ENERGY).child(energy_tag(spec.energy)))
            })
            .child(match spec.quit {
                Some((identity, name)) => cell_right(W_ACTION)
                    .opacity(0.0)
                    .group_hover("proc-row", |s| s.opacity(1.0))
                    .child(
                        button(
                            SharedString::from(format!("{}-quit", spec.id)),
                            "Quit",
                            None,
                            Variant::Danger,
                            true,
                        )
                        .tooltip(|_, cx| tip(crate::words::QUIT_TIP.into(), cx))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.quit_process(identity, name.clone(), "SIGTERM", cx)
                        })),
                    ),
                None => div().w(px(W_ACTION)).flex_shrink_0(),
            })
    }

    /// The rows on screen, and only those.
    fn process_rows(&mut self, range: Range<usize>, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let pal = p();
        let rows = self.proc_rows.clone();
        let sorted = self.proc_sort;
        let scale = self.proc_ui.scale;
        let lines: Vec<(usize, Line)> = range
            .filter_map(|i| self.proc_ui.lines.get(i).map(|l| (i, l.clone())))
            .collect();
        lines
            .into_iter()
            .filter_map(|(i, line)| match line {
                Line::Heading { application, count } => Some(
                    div()
                        .w_full()
                        .h(px(ROW_H))
                        .px(px(S3))
                        .flex()
                        .items_center()
                        .font(body(FontWeight::SEMIBOLD))
                        .text_size(px(T_META))
                        .text_color(pal.text_2)
                        .child(format!(
                            "{} ({})",
                            if application {
                                "Applications"
                            } else {
                                "Background"
                            },
                            format_count(count as u64)
                        ))
                        .into_any_element(),
                ),
                Line::Group { group, open } => {
                    let g = self.proc_ui.groups.get(group)?.clone();
                    let several = g.members.len() > 1;
                    let first = rows.processes.get(*g.members.first()?)?;
                    let pid = g
                        .members
                        .iter()
                        .filter_map(|&m| rows.processes.get(m))
                        .map(|r| r.identity.pid)
                        .min()
                        .unwrap_or(first.identity.pid);
                    let spec = RowSpec {
                        id: SharedString::from(format!("proc-group-{i}")),
                        chevron: several.then_some(open),
                        indent: false,
                        icon: app_icon(&self.hub.icons, g.bundle.as_deref(), ICON_PX),
                        name: g.name.clone(),
                        detail: several.then(|| format!("{} processes", g.members.len())),
                        tip: SharedString::from(
                            g.bundle
                                .clone()
                                .unwrap_or_else(|| first.executable_path.clone()),
                        ),
                        is_self: g
                            .members
                            .iter()
                            .any(|&m| rows.processes.get(m).is_some_and(|r| r.is_self)),
                        user: (!several).then(|| named_user(&first.user)).flatten(),
                        pid,
                        cpu_pct: g.cpu_pct,
                        memory_bytes: g.memory_bytes,
                        disk_rate: g.disk_bytes_per_sec,
                        energy: g.energy,
                        quit: (!several).then(|| (first.identity, first.name.clone())),
                    };
                    let row = self.line_row(spec, sorted, scale, cx);
                    Some(if several {
                        let key = g.key.clone();
                        row.cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if !this.proc_ui.expanded.remove(&key) {
                                    this.proc_ui.expanded.insert(key.clone());
                                }
                                cx.notify();
                            }))
                            .into_any_element()
                    } else {
                        row.into_any_element()
                    })
                }
                Line::Member { group: _, row } => {
                    let r = rows.processes.get(row)?;
                    let spec = RowSpec {
                        id: SharedString::from(format!("proc-{}", r.identity.pid)),
                        chevron: None,
                        indent: true,
                        icon: app_icon(&self.hub.icons, None, ICON_PX),
                        name: r.name.clone(),
                        detail: None,
                        tip: SharedString::from(if r.executable_path.is_empty() {
                            r.name.clone()
                        } else {
                            r.executable_path.clone()
                        }),
                        is_self: r.is_self,
                        user: named_user(&r.user),
                        pid: r.identity.pid,
                        cpu_pct: r.cpu_pct,
                        memory_bytes: r.memory_bytes,
                        disk_rate: r.disk_read_bytes_per_sec + r.disk_write_bytes_per_sec,
                        energy: r.energy_impact,
                        quit: Some((r.identity, r.name.clone())),
                    };
                    Some(self.line_row(spec, sorted, scale, cx).into_any_element())
                }
            })
            .collect()
    }

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

        // Apps, then the list: every process was read, so an app's totals cover all its helpers.
        let query = self.proc_ui.search.read(cx).text().to_string();
        let searching = !query.trim().is_empty();
        let (applications, apps_known) = self.hub.icons.applications();
        let mut groups = group_processes_on(&rows.processes, &applications, cfg!(windows));
        if cfg!(windows) {
            // Windows names a program in its .exe ("Antimalware Service Executable" for MsMpEng.exe). Asking for
            // every app's icon now loads every name too, so the list sorts by them, not only the rows on screen.
            for group in &mut groups {
                if let Some(app) = group.bundle.as_deref() {
                    let _ = self.hub.icons.icon(app);
                    if let Some(title) = self.hub.icons.title(app) {
                        group.name = title;
                    }
                }
            }
        }
        sort_groups(&mut groups, &rows.processes, self.proc_sort.as_str());
        let visible: Vec<Vec<usize>> = groups
            .iter()
            .map(|g| visible_members(g, &rows.processes, &query))
            .collect();
        let lines = layout_lines(
            &groups,
            &visible,
            &self.proc_ui.expanded,
            searching,
            apps_known,
        );
        let shown = visible.iter().filter(|v| !v.is_empty()).count();
        let cpu_scale = groups.iter().map(|g| g.cpu_pct).fold(CPU_FLOOR, f32::max);
        let mem_scale = groups
            .iter()
            .map(|g| g.memory_bytes)
            .fold(MEM_FLOOR, u64::max);
        let count = lines.len();
        self.proc_ui.lines = lines;
        self.proc_ui.groups = groups;
        self.proc_ui.scale = (cpu_scale, mem_scale);

        let sub = if searching {
            format!(
                "{} {} for “{}”.",
                format_count(shown as u64),
                if shown == 1 { "match" } else { "matches" },
                query.trim()
            )
        } else if apps_known {
            format!(
                "{} running. Click an app to see its processes.",
                format_count(rows.total_processes as u64)
            )
        } else {
            format!("{} running.", format_count(rows.total_processes as u64))
        };

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

        let list: AnyElement = if count == 0 {
            div()
                .p(px(S3))
                .child(if searching {
                    empty(&format!("Nothing matches “{}”.", query.trim()))
                } else {
                    loading("Reading the process list…")
                })
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
                actions: vec![
                    div()
                        .w(px(260.0))
                        .child(self.proc_ui.search.clone())
                        .into_any_element(),
                    toggle.into_any_element(),
                ],
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

/// A named user (root, _windowserver) worth showing; a bare uid says nothing.
fn named_user(user: &str) -> Option<String> {
    (!user.chars().all(|c| c.is_ascii_digit())).then(|| user.to_string())
}

/// `value / whole`, 0 to 1, and 0 for an empty whole.
fn ratio_f(value: f32, whole: f32) -> f32 {
    if whole <= 0.0 {
        0.0
    } else {
        (value / whole).clamp(0.0, 1.0)
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

    fn group(name: &str, members: &[usize], application: bool) -> ProcessGroup {
        ProcessGroup {
            key: GroupKey::App(format!("/Applications/{name}.app")),
            name: name.to_string(),
            bundle: Some(format!("/Applications/{name}.app")),
            application,
            members: members.to_vec(),
            cpu_pct: 0.0,
            memory_bytes: 0,
            disk_bytes_per_sec: 0,
            energy: EnergyImpact::Low,
        }
    }

    /// Two apps (one of three processes, one alone) and a background process.
    fn sample() -> (Vec<ProcessGroup>, Vec<Vec<usize>>) {
        let groups = vec![
            group("Big", &[0, 1, 2], true),
            group("Small", &[3], true),
            group("daemon", &[4], false),
        ];
        let visible = groups.iter().map(|g| g.members.clone()).collect();
        (groups, visible)
    }

    #[test]
    fn applications_come_first_each_under_its_heading() {
        let (groups, visible) = sample();
        let lines = layout_lines(&groups, &visible, &HashSet::new(), false, true);
        assert_eq!(
            lines,
            [
                Line::Heading {
                    application: true,
                    count: 2
                },
                Line::Group {
                    group: 0,
                    open: false
                },
                Line::Group {
                    group: 1,
                    open: false
                },
                Line::Heading {
                    application: false,
                    count: 1
                },
                Line::Group {
                    group: 2,
                    open: false
                },
            ]
        );
    }

    #[test]
    fn an_open_app_is_followed_by_its_processes() {
        let (groups, visible) = sample();
        let open = HashSet::from([groups[0].key.clone()]);
        let lines = layout_lines(&groups, &visible, &open, false, true);
        assert_eq!(
            &lines[1..5],
            [
                Line::Group {
                    group: 0,
                    open: true
                },
                Line::Member { group: 0, row: 0 },
                Line::Member { group: 0, row: 1 },
                Line::Member { group: 0, row: 2 },
            ]
        );
    }

    #[test]
    fn an_app_of_one_process_has_nothing_to_open() {
        let (groups, visible) = sample();
        // Even if it were marked open (it can't be: there is no chevron), it shows no members.
        let open = HashSet::from([groups[1].key.clone()]);
        let lines = layout_lines(&groups, &visible, &open, false, true);
        assert!(
            !lines
                .iter()
                .any(|l| matches!(l, Line::Member { group: 1, .. }))
        );
        assert!(lines.contains(&Line::Group {
            group: 1,
            open: false
        }));
    }

    #[test]
    fn typing_opens_every_app_and_shows_only_what_matches() {
        let (groups, mut visible) = sample();
        visible[0] = vec![1]; // only one of Big's processes matches
        visible[1] = vec![]; // Small doesn't match: hidden
        let lines = layout_lines(&groups, &visible, &HashSet::new(), true, true);
        assert!(lines.contains(&Line::Group {
            group: 0,
            open: true
        }));
        assert!(lines.contains(&Line::Member { group: 0, row: 1 }));
        assert!(!lines.contains(&Line::Member { group: 0, row: 0 }));
        assert!(
            !lines
                .iter()
                .any(|l| matches!(l, Line::Group { group: 1, .. }))
        );
        // The headings count what is shown: one application now.
        assert_eq!(
            lines[0],
            Line::Heading {
                application: true,
                count: 1
            }
        );
    }

    #[test]
    fn a_section_with_nothing_in_it_has_no_heading() {
        let (groups, mut visible) = sample();
        visible[2] = vec![];
        let lines = layout_lines(&groups, &visible, &HashSet::new(), false, true);
        assert!(!lines.contains(&Line::Heading {
            application: false,
            count: 0
        }));
        assert!(!lines.iter().any(|l| matches!(
            l,
            Line::Heading {
                application: false,
                ..
            }
        )));
    }

    #[test]
    fn where_the_os_has_not_said_which_are_applications_it_is_one_plain_list() {
        let (groups, visible) = sample();
        let lines = layout_lines(&groups, &visible, &HashSet::new(), false, false);
        assert!(!lines.iter().any(|l| matches!(l, Line::Heading { .. })));
        assert_eq!(lines.len(), 3, "one row per group, none hidden");
    }

    #[test]
    fn bars_are_a_share_of_their_scale() {
        assert_eq!(ratio_f(50.0, 100.0), 0.5);
        assert_eq!(ratio_f(500.0, 100.0), 1.0, "never past full");
        assert_eq!(ratio_f(5.0, 0.0), 0.0, "no scale, no bar");
        assert_eq!(named_user("501"), None);
        assert_eq!(named_user("root").as_deref(), Some("root"));
    }
}
