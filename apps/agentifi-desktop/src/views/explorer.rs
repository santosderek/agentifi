//! Explorer: the durable session inventory.
//!
//! Project tree on the left, a dense session list in the middle, and an optional
//! inspector on the right. Filters, sort, search, and selection survive
//! navigation to and from the workspace.

use crate::{
    components::{
        empty_state_action, icon_button, inspector, rail_project, session_row, text, toolbar, Icon,
        RowStyle,
    },
    state::{AppView, SortKey},
    theme,
    views::{Action, ViewContext},
};
use agentifi_domain::SessionStatus;
use eframe::egui::{Align, Layout, RichText, ScrollArea, Ui, Vec2};

pub const LABEL: &str = "Explorer";

const TREE_WIDTH: f32 = 196.0;
const INSPECTOR_WIDTH: f32 = 268.0;

pub fn show(ui: &mut Ui, ctx: &mut ViewContext<'_>) {
    let visible = ctx.state.visible(ctx.sessions);
    header(ui, ctx, visible.len());
    ui.add_space(theme::SPACE_MD);
    theme::hairline(ui);
    ui.add_space(theme::SPACE_MD);

    let height = ui.available_height();
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            Vec2::new(TREE_WIDTH, height),
            Layout::top_down(Align::Min),
            |ui| project_tree(ui, ctx),
        );
        ui.add_space(theme::SPACE_LG);

        let inspector_space = if ctx.state.inspector_open {
            INSPECTOR_WIDTH + theme::SPACE_LG
        } else {
            0.0
        };
        let list_width = (ui.available_width() - inspector_space).max(320.0);
        ui.allocate_ui_with_layout(
            Vec2::new(list_width, height),
            Layout::top_down(Align::Min),
            |ui| session_list(ui, ctx, &visible),
        );

        if ctx.state.inspector_open {
            ui.add_space(theme::SPACE_LG);
            ui.allocate_ui_with_layout(
                Vec2::new(INSPECTOR_WIDTH, height),
                Layout::top_down(Align::Min),
                |ui| side_inspector(ui, ctx),
            );
        }
    });
}

fn header(ui: &mut Ui, ctx: &mut ViewContext<'_>, count: usize) {
    toolbar::title_block(
        ui,
        "Explorer",
        "Search, filter, and open any discovered Pi session",
    );
    ui.add_space(theme::SPACE_MD);
    ui.horizontal(|ui| {
        crate::components::paint(ui, Icon::Search, theme::TEXT_MUTED, 14.0);
        let focus = std::mem::take(&mut ctx.state.focus_search);
        toolbar::search_field(
            ui,
            &mut ctx.state.search,
            "Search titles, prompts, projects, branches, models…",
            320.0,
            focus,
        );

        status_menu(ui, ctx);
        project_menu(ui, ctx);
        model_menu(ui, ctx);
        sort_menu(ui, ctx);
        if ctx.state.has_filters() && ui.button("Clear").clicked() {
            ctx.state.clear_filters();
        }

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if icon_button(ui, Icon::Refresh, "Refresh catalog").clicked() {
                ctx.act(Action::Refresh);
            }
            let label = if ctx.state.inspector_open {
                "Hide inspector"
            } else {
                "Show inspector"
            };
            if ui.button(label).clicked() {
                ctx.state.inspector_open = !ctx.state.inspector_open;
            }
            toolbar::count_label(ui, count, "results");
        });
    });
}

fn status_menu(ui: &mut Ui, ctx: &mut ViewContext<'_>) {
    let current = ctx.state.status_filter.map(SessionStatus::label);
    toolbar::filter_menu(ui, "Status", current, |ui| {
        if ui.button("All statuses").clicked() {
            ctx.state.status_filter = None;
            ui.close_menu();
        }
        for status in SessionStatus::ALL {
            if ui.button(status.label()).clicked() {
                ctx.state.status_filter = Some(status);
                ui.close_menu();
            }
        }
    });
}

fn project_menu(ui: &mut Ui, ctx: &mut ViewContext<'_>) {
    let projects = ctx.projects();
    let current = ctx.state.project_filter.clone();
    toolbar::filter_menu(ui, "Project", current.as_deref(), |ui| {
        if ui.button("All projects").clicked() {
            ctx.state.project_filter = None;
            ui.close_menu();
        }
        for (project, count) in projects {
            if ui.button(format!("{project}  ({count})")).clicked() {
                ctx.state.project_filter = Some(project);
                ui.close_menu();
            }
        }
    });
}

fn model_menu(ui: &mut Ui, ctx: &mut ViewContext<'_>) {
    let models = ctx.models();
    let current = ctx.state.model_filter.clone();
    toolbar::filter_menu(ui, "Model", current.as_deref(), |ui| {
        if ui.button("All models").clicked() {
            ctx.state.model_filter = None;
            ui.close_menu();
        }
        if models.is_empty() {
            ui.label(
                RichText::new("No model reported yet")
                    .font(theme::font_meta())
                    .color(theme::TEXT_MUTED),
            );
        }
        for model in models {
            if ui.button(&model).clicked() {
                ctx.state.model_filter = Some(model);
                ui.close_menu();
            }
        }
    });
}

fn sort_menu(ui: &mut Ui, ctx: &mut ViewContext<'_>) {
    ui.menu_button(
        RichText::new(format!("Sort: {}", ctx.state.sort.label())).font(theme::font_meta()),
        |ui| {
            for key in SortKey::ALL {
                if ui.button(key.label()).clicked() {
                    ctx.state.sort = key;
                    ui.close_menu();
                }
            }
        },
    );
}

fn project_tree(ui: &mut Ui, ctx: &mut ViewContext<'_>) {
    text::section_label(ui, "Projects");
    let all_selected = ctx.state.project_filter.is_none();
    if rail_project(ui, "All projects", ctx.sessions.len(), all_selected) {
        ctx.state.project_filter = None;
    }
    ScrollArea::vertical()
        .id_salt("explorer-projects")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (project, count) in ctx.projects() {
                let selected = ctx.state.project_filter.as_deref() == Some(project.as_str());
                if rail_project(ui, &project, count, selected) {
                    ctx.state.project_filter = if selected { None } else { Some(project) };
                }
            }

            let statuses: Vec<(SessionStatus, usize)> = SessionStatus::ALL
                .into_iter()
                .map(|status| {
                    (
                        status,
                        ctx.sessions
                            .iter()
                            .filter(|session| session.status == status)
                            .count(),
                    )
                })
                .filter(|(_, count)| *count > 0)
                .collect();
            if !statuses.is_empty() {
                ui.add_space(theme::SPACE_LG);
                text::section_label(ui, "Status");
                for (status, count) in statuses {
                    let selected = ctx.state.status_filter == Some(status);
                    if rail_project(ui, status.label(), count, selected) {
                        ctx.state.status_filter = if selected { None } else { Some(status) };
                    }
                }
            }
        });
}

fn session_list(ui: &mut Ui, ctx: &mut ViewContext<'_>, visible: &[agentifi_domain::AgentSession]) {
    if visible.is_empty() {
        let action = if ctx.state.has_filters() {
            "Clear filters"
        } else {
            "Refresh catalog"
        };
        if empty_state_action(
            ui,
            "No sessions match",
            "Adjust the search or filters, or refresh the catalog to pick up new Pi sessions.",
            action,
        ) {
            if ctx.state.has_filters() {
                ctx.state.clear_filters();
            } else {
                ctx.act(Action::Refresh);
            }
        }
        return;
    }

    ScrollArea::vertical()
        .id_salt("explorer-list")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for session in visible {
                let selected = ctx.state.selected == Some(session.id);
                let response = session_row(ui, session, selected, ctx.now, RowStyle::Full);
                if response.clicked() {
                    ctx.state.selected = Some(session.id);
                    ctx.state.inspector_open = true;
                }
                if response.double_clicked() {
                    ctx.state.open_session(session.id);
                    ctx.act(Action::Attach(session.id));
                }
                response.context_menu(|ui| {
                    if ui.button("Open workspace").clicked() {
                        ctx.state.open_session(session.id);
                        ctx.act(Action::Attach(session.id));
                        ui.close_menu();
                    }
                    if ui.button("Filter to this project").clicked() {
                        ctx.state.project_filter = Some(session.project.clone());
                        ui.close_menu();
                    }
                    if ui.button("Copy session ID").clicked() {
                        ui.ctx().copy_text(session.id.to_string());
                        ui.close_menu();
                    }
                });
            }
        });
}

fn side_inspector(ui: &mut Ui, ctx: &mut ViewContext<'_>) {
    theme::panel().show(ui, |ui| {
        let Some(session) = ctx.selected().cloned() else {
            text::section_label(ui, "Inspector");
            crate::components::empty_state(
                ui,
                "Nothing selected",
                "Select a session to see its project, model, and activity. Double-click to open it.",
            );
            return;
        };

        text::section_label(ui, "Selected session");
        ui.label(
            RichText::new(session.display_title())
                .font(theme::font_row_title())
                .color(theme::TEXT_PRIMARY),
        );
        if let Some(summary) = session.summary.as_deref() {
            ui.label(
                RichText::new(summary)
                    .font(theme::font_meta())
                    .color(theme::TEXT_SECONDARY),
            );
        }
        ui.add_space(theme::SPACE_MD);

        let tab = inspector::tab_strip(ui, ctx.state.inspector_tab);
        ctx.state.inspector_tab = tab;
        ui.add_space(theme::SPACE_SM);
        let session_events: Vec<_> = ctx
            .events
            .iter()
            .filter(|event| event.belongs_to(&session.id.to_string()))
            .cloned()
            .collect();
        inspector::inspector_body(ui, &session, tab, &session_events, ctx.now);

        ui.add_space(theme::SPACE_LG);
        ui.horizontal(|ui| {
            if ui.button("Open workspace").clicked() {
                ctx.state.open_session(session.id);
                ctx.act(Action::Attach(session.id));
            }
            if ui.button("Board").clicked() {
                ctx.state.go(AppView::Board);
            }
        });
    });
}
