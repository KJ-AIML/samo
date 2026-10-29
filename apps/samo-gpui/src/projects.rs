//! Projects: every detected project root, what it is made of, and the build artifacts Samo suggests
//! clearing. A suggestion is dithered until you act on it; nothing moves until you say so.

use crate::art::{dither_block, dither_frame, split_bar};
use crate::components::*;
use crate::overview::{figure_cell, list_row};
use crate::shell::{Head, Page, pending, scan_sub};
use crate::theme::*;
use crate::{Frame, SamoWorkspaceApp};
use gpui::{
    Context, Div, FontWeight, IntoElement, ParentElement, SharedString, Stateful, Styled, div,
    prelude::*, px,
};
use samo_primitives::ProjectSummary;

fn legend() -> Div {
    let pal = p();
    let item = |swatch: gpui::AnyElement, label: &str| {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(S2))
            .font(body(FontWeight::NORMAL))
            .text_size(px(T_META))
            .text_color(pal.text_2)
            .child(swatch)
            .child(label.to_string())
    };
    div()
        .flex()
        .flex_row()
        .gap(px(S4))
        .child(item(
            div().size(px(10.0)).bg(pal.text_2).into_any_element(),
            "Source",
        ))
        .child(item(
            dither_block(10.0, 10.0, pal.accent).into_any_element(),
            "Samo suggests clearing",
        ))
}

impl SamoWorkspaceApp {
    pub(crate) fn render_projects(&self, frame: &Frame, cx: &mut Context<Self>) -> Page {
        let sub = scan_sub(frame, |d| {
            let reclaimable: u64 = d.projects.iter().map(|p| p.artifact_bytes).sum();
            format!(
                "{} project{} found; Samo suggests clearing {} of build artifacts.",
                d.projects.len(),
                if d.projects.len() == 1 { "" } else { "s" },
                format_bytes(reclaimable)
            )
        });

        let roomy = frame.layout.roomy();
        let body = match frame.view() {
            None => pending(frame),
            Some(view) if view.snap.projects.is_empty() => empty(&format!(
                "No projects under {}. Samo looks for Cargo.toml, package.json, pyproject.toml, go.mod and .git. Try Developer in the sidebar.",
                display_path(&view.snap.root_path)
            )),
            Some(view) => {
                let widest = view.widest_project.max(1);
                div()
                    .flex()
                    .flex_col()
                    .children(view.projects_ranked.iter().enumerate().map(|(i, &pi)| {
                        self.project_row(i, &view.snap.projects[pi], widest, roomy, cx)
                    }))
            }
        };

        Page {
            head: Head {
                icon: "package",
                title: "Projects".to_string(),
                sub,
                crumbs: true,
                actions: self.disk_actions(frame, cx),
            },
            body: div().flex().flex_col().gap(px(S4)).child(
                card()
                    .child(card_head(
                        "Projects",
                        frame.current_disk().map(|d| d.projects.len().to_string()),
                        Some(legend().into_any_element()),
                    ))
                    .child(body),
            ),
            fill: false,
        }
    }

    fn project_row(
        &self,
        i: usize,
        proj: &ProjectSummary,
        widest: u64,
        roomy: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let size_w = if roomy { 150.0 } else { 124.0 };
        let pal = p();
        let open_path = proj.root_path.clone();
        let reveal_path = proj.root_path.clone();

        list_row(SharedString::from(format!("proj-{i}")), "project-row")
            .py(px(S4))
            .flex_col()
            .items_start()
            .gap(px(S3))
            .border_b_1()
            .border_color(pal.hairline)
            .rounded(px(0.0))
            .child(
                div()
                    .w_full()
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
                            .gap(px(S1))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap(px(S2))
                                    .child(
                                        line(
                                            proj.name.clone(),
                                            FontWeight::SEMIBOLD,
                                            T_BODY,
                                            pal.text,
                                        )
                                        .flex_1(),
                                    )
                                    .child(tag(proj.kind.clone(), TagKind::Neutral, false)),
                            )
                            .child(
                                mono_line(display_path(&proj.root_path), 12.5, pal.text_2).w_full(),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap(px(2.0))
                            .opacity(0.0)
                            .group_hover("project-row", |s| s.opacity(1.0))
                            .child(
                                icon_button(
                                    SharedString::from(format!("proj-open-{i}")),
                                    "folder",
                                    "Measure this project",
                                    Variant::Link,
                                    true,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| this.trigger_scan(open_path.clone(), cx),
                                )),
                            )
                            .child(
                                icon_button(
                                    SharedString::from(format!("proj-reveal-{i}")),
                                    "reveal",
                                    "Reveal in Finder",
                                    Variant::Link,
                                    true,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.reveal_in_finder(reveal_path.clone(), cx)
                                    },
                                )),
                            ),
                    )
                    .child(
                        div()
                            .w(px(size_w))
                            .flex_shrink_0()
                            .flex()
                            .flex_col()
                            .items_end()
                            .child(figure_cell(format_bytes(proj.total_bytes), size_w, true))
                            .child(figure_cell(
                                if proj.artifact_bytes > 0 {
                                    format!("{} to clear", format_bytes(proj.artifact_bytes))
                                } else {
                                    "nothing to clear".to_string()
                                },
                                size_w,
                                false,
                            )),
                    ),
            )
            .child(split_bar(
                proj.source_bytes as f32,
                proj.artifact_bytes as f32,
                ratio(proj.total_bytes, widest).max(0.02),
                8.0,
            ))
            .when(!proj.artifacts.is_empty(), |el| {
                el.child(div().flex().flex_row().flex_wrap().gap(px(S2)).children(
                    proj.artifacts.iter().enumerate().map(|(a_i, art)| {
                        let label = format!("{}/{}", proj.name, art.name);
                        let path = art.path.clone();
                        let bytes = art.bytes;
                        let identity = art.identity;
                        let tooltip_text = SharedString::from(format!(
                            "Move {} to the Trash…",
                            display_path(&art.path)
                        ));
                        // a suggestion: the chip wears the dither until you act on it
                        div()
                            .id(SharedString::from(format!("proj-art-{i}-{a_i}")))
                            .relative()
                            .h(px(36.0))
                            .pl(px(S3))
                            .pr(px(S3 + 2.0))
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(S2))
                            .rounded(px(R_CHIP))
                            .bg(pal.card)
                            .cursor_pointer()
                            .hover(|s| s.bg(pal.accent_wash))
                            .tooltip(move |_, cx| tip(tooltip_text.clone(), cx))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.request_trash_confirmation(
                                    label.clone(),
                                    path.clone(),
                                    bytes,
                                    identity,
                                    cx,
                                )
                            }))
                            .child(icon("trash", 14.0, pal.accent_text))
                            .child(
                                div()
                                    .font(body(FontWeight::MEDIUM))
                                    .text_size(px(14.0))
                                    .text_color(pal.text)
                                    .child(art.name.clone()),
                            )
                            .child(
                                div()
                                    .font(mono(FontWeight::NORMAL))
                                    .text_size(px(12.5))
                                    .text_color(pal.text_2)
                                    .child(format_bytes(art.bytes)),
                            )
                            .child(dither_frame(R_CHIP, pal.accent))
                    }),
                ))
            })
    }
}
