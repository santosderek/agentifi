//! Session workspace: the screen a developer actually works in.
//!
//! Layout: context rail (project, branch, sibling sessions) · timeline · inspector,
//! with the prompt composer pinned to the bottom. The composer's primary action
//! follows the session's real state so the user never guesses what Enter does.

use crate::{
    components::{
        activity_timeline, empty_state_action, icon_button, inspector, prompt_composer,
        session_row, text, toolbar, ComposerAction, Icon, RowStyle,
    },
    state::{AppView, TimelineFilter},
    theme,
    views::{Action, ViewContext},
};
use agentifi_domain::AgentSession;
use eframe::egui::{Align, Align2, Layout, Pos2, RichText, ScrollArea, Sense, Ui, Vec2};

pub const LABEL: &str = "Workspace";

const RAIL_WIDTH: f32 = 218.0;
const INSPECTOR_WIDTH: f32 = 274.0;
const COMPOSER_HEIGHT: f32 = 150.0;

pub fn show(ui: &mut Ui, ctx: &mut ViewContext<'_>) {
    let Some(session) = ctx.selected().cloned() else {
        toolbar::title_block(ui, "Workspace", "No session attached");
        ui.add_space(theme::SPACE_LG);
        if empty_state_action(
            ui,
            "No session selected",
            "Pick a session in the Explorer to attach to its Pi process and stream its transcript.",
            "Open Explorer",
        ) {
            ctx.state.go(AppView::Explorer);
        }
        return;
    };

    header(ui, ctx, &session);
    ui.add_space(theme::SPACE_MD);
    theme::hairline(ui);
    ui.add_space(theme::SPACE_MD);

    let events: Vec<_> = ctx
        .events
        .iter()
        .filter(|event| event.belongs_to(&session.id.to_string()))
        .cloned()
        .collect();

    let body_height = (ui.available_height() - COMPOSER_HEIGHT - theme::SPACE_LG).max(220.0);
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            Vec2::new(RAIL_WIDTH, body_height),
            Layout::top_down(Align::Min),
            |ui| context_rail(ui, ctx, &session),
        );
        ui.add_space(theme::SPACE_LG);

        let inspector_space = if ctx.state.inspector_open {
            INSPECTOR_WIDTH + theme::SPACE_LG
        } else {
            0.0
        };
        let timeline_width = (ui.available_width() - inspector_space).max(320.0);
        ui.allocate_ui_with_layout(
            Vec2::new(timeline_width, body_height),
            Layout::top_down(Align::Min),
            |ui| timeline(ui, ctx, &events, body_height),
        );

        if ctx.state.inspector_open {
            ui.add_space(theme::SPACE_LG);
            ui.allocate_ui_with_layout(
                Vec2::new(INSPECTOR_WIDTH, body_height),
                Layout::top_down(Align::Min),
                |ui| {
                    theme::panel().show(ui, |ui| {
                        text::section_label(ui, "Inspector");
                        let tab = inspector::tab_strip(ui, ctx.state.inspector_tab);
                        ctx.state.inspector_tab = tab;
                        ui.add_space(theme::SPACE_SM);
                        inspector::inspector_body(ui, &session, tab, &events, ctx.now);
                    });
                },
            );
        }
    });

    ui.add_space(theme::SPACE_LG);
    composer(ui, ctx, &session);
}

fn header(ui: &mut Ui, ctx: &mut ViewContext<'_>, session: &AgentSession) {
    toolbar::breadcrumb(
        ui,
        &[
            "Explorer".to_owned(),
            session.project.clone(),
            session.display_title(),
        ],
    );
    ui.add_space(theme::SPACE_XS);
    ui.horizontal(|ui| {
        if icon_button(ui, Icon::Back, "Back to the previous view").clicked() {
            ctx.state.back();
        }
        ui.label(
            RichText::new(session.display_title())
                .font(theme::font_title())
                .color(theme::TEXT_PRIMARY),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let label = if ctx.state.inspector_open {
                "Hide inspector"
            } else {
                "Show inspector"
            };
            if ui.button(label).clicked() {
                ctx.state.inspector_open = !ctx.state.inspector_open;
            }
            if ui.button("Reattach").clicked() {
                ctx.act(Action::Attach(session.id));
            }
        });
    });
    ui.add_space(theme::SPACE_XS);
    let summary = crate::components::status_badge::header_summary(session, session.attachment);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 18.0), Sense::hover());
    text::line(
        ui.painter(),
        rect.left_center(),
        Align2::LEFT_CENTER,
        &format!("{summary}  ·  {}", session.age_label(ctx.now)),
        theme::font_meta(),
        theme::TEXT_SECONDARY,
        rect.width(),
    );
}

fn context_rail(ui: &mut Ui, ctx: &mut ViewContext<'_>, session: &AgentSession) {
    theme::panel().show(ui, |ui| {
        text::section_label(ui, "Context");
        inspector::row(ui, "Project", &session.project);
        inspector::row(
            ui,
            "Branch",
            session.branch.as_deref().unwrap_or("not reported"),
        );
        inspector::row(
            ui,
            "Directory",
            session
                .working_directory
                .as_deref()
                .unwrap_or("not reported"),
        );
        inspector::row(
            ui,
            "Model",
            &session
                .model_label()
                .unwrap_or_else(|| "not reported".into()),
        );

        ui.add_space(theme::SPACE_LG);
        text::section_label(ui, "Sessions in project");
        let siblings: Vec<AgentSession> = ctx
            .sessions
            .iter()
            .filter(|other| other.project == session.project)
            .take(12)
            .cloned()
            .collect();
        ScrollArea::vertical()
            .id_salt("workspace-siblings")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for sibling in siblings {
                    let selected = sibling.id == session.id;
                    if session_row(ui, &sibling, selected, ctx.now, RowStyle::Minimal).clicked() {
                        ctx.state.open_session(sibling.id);
                        ctx.act(Action::Attach(sibling.id));
                    }
                }
            });
    });
}

fn timeline(
    ui: &mut Ui,
    ctx: &mut ViewContext<'_>,
    events: &[crate::events::ActivityEvent],
    height: f32,
) {
    ui.horizontal(|ui| {
        text::section_label(ui, "Transcript");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            for filter in TimelineFilter::ALL {
                if ui
                    .selectable_label(
                        ctx.state.timeline_filter == filter,
                        RichText::new(filter.label()).font(theme::font_meta()),
                    )
                    .clicked()
                {
                    ctx.state.timeline_filter = filter;
                }
            }
        });
    });
    ui.add_space(theme::SPACE_SM);
    activity_timeline(
        ui,
        events,
        ctx.state.timeline_filter,
        (height - 44.0).max(160.0),
    );
}

fn composer(ui: &mut Ui, ctx: &mut ViewContext<'_>, session: &AgentSession) {
    let action = prompt_composer(
        ui,
        session,
        &mut ctx.state.composer,
        &mut ctx.state.confirm_abort,
    );
    let text = ctx.state.composer.trim().to_owned();
    match action {
        ComposerAction::None => {}
        ComposerAction::Attach => ctx.act(Action::Attach(session.id)),
        ComposerAction::Send if !text.is_empty() => {
            ctx.state.composer.clear();
            ctx.act(Action::Prompt(session.id, text));
        }
        ComposerAction::FollowUp if !text.is_empty() => {
            ctx.state.composer.clear();
            ctx.act(Action::FollowUp(session.id, text));
        }
        ComposerAction::Steer if !text.is_empty() => {
            ctx.state.composer.clear();
            ctx.act(Action::Steer(session.id, text));
        }
        ComposerAction::Abort => ctx.act(Action::Abort(session.id)),
        _ => ctx.state.notify(
            crate::state::Tone::Warning,
            "Write a prompt first".to_owned(),
        ),
    }
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 16.0), Sense::hover());
    text::line(
        ui.painter(),
        Pos2::new(rect.left(), rect.center().y),
        Align2::LEFT_CENTER,
        "Ctrl+Enter send  ·  Ctrl+S steer  ·  Esc back",
        theme::font_label(),
        theme::TEXT_MUTED,
        rect.width(),
    );
}
