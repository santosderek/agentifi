//! Board: operational triage.
//!
//! Lanes describe what the user needs to do with a session; the card's status dot
//! keeps describing what Pi is doing. Moving a card never changes the Pi process.

use crate::{
    components::{board_card, empty_state, icon_button, text, toolbar, Icon},
    state::AppView,
    theme,
    views::{Action, ViewContext},
};
use agentifi_domain::{BoardLane, SessionStatus};
use eframe::egui::{self, Align, Layout, RichText, ScrollArea, Ui};

pub const LABEL: &str = "Board";

pub fn show(ui: &mut Ui, ctx: &mut ViewContext<'_>) {
    let visible = ctx.state.visible(ctx.sessions);
    header(ui, ctx, visible.len());
    ui.add_space(theme::SPACE_MD);
    theme::hairline(ui);
    ui.add_space(theme::SPACE_MD);

    let lanes = BoardLane::ALL;
    ui.columns(lanes.len(), |columns| {
        for (index, lane) in lanes.into_iter().enumerate() {
            let ui = &mut columns[index];
            let cards: Vec<_> = visible
                .iter()
                .filter(|session| ctx.state.lane_of(session) == lane)
                .cloned()
                .collect();
            lane_header(ui, ctx, lane, cards.len());

            ScrollArea::vertical()
                .id_salt(format!("board-lane-{index}"))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if cards.is_empty() {
                        empty_state(
                            ui,
                            "Empty lane",
                            "Move a card here with [ and ] once a session is selected.",
                        );
                        return;
                    }
                    for session in cards {
                        let selected = ctx.state.selected == Some(session.id);
                        let response = board_card(ui, &session, selected, ctx.now);
                        if response.clicked() {
                            ctx.state.selected = Some(session.id);
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
                            ui.separator();
                            for target in BoardLane::ALL {
                                if target != lane
                                    && ui.button(format!("Move to {}", target.label())).clicked()
                                {
                                    ctx.state.set_lane(session.id, target);
                                    ui.close_menu();
                                }
                            }
                        });
                    }
                });
        }
    });
}

fn header(ui: &mut Ui, ctx: &mut ViewContext<'_>, count: usize) {
    toolbar::title_block(
        ui,
        "Board",
        "Organise agent work by operational state · lanes are yours, status dots are Pi's",
    );
    ui.add_space(theme::SPACE_MD);
    ui.horizontal(|ui| {
        let focus = std::mem::take(&mut ctx.state.focus_search);
        toolbar::search_field(ui, &mut ctx.state.search, "Search board…", 260.0, focus);
        let project = ctx.state.project_filter.clone();
        let projects = ctx.projects();
        toolbar::filter_menu(ui, "Project", project.as_deref(), |ui| {
            if ui.button("All projects").clicked() {
                ctx.state.project_filter = None;
                ui.close_menu();
            }
            for (name, total) in projects {
                if ui.button(format!("{name}  ({total})")).clicked() {
                    ctx.state.project_filter = Some(name);
                    ui.close_menu();
                }
            }
        });
        ui.label(
            RichText::new("[  ]  move selected card")
                .font(theme::font_meta())
                .color(theme::TEXT_MUTED),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if icon_button(ui, Icon::Refresh, "Refresh catalog").clicked() {
                ctx.act(Action::Refresh);
            }
            if ui.button("Explorer").clicked() {
                ctx.state.go(AppView::Explorer);
            }
            toolbar::count_label(ui, count, "cards");
        });
    });
}

fn lane_header(ui: &mut Ui, ctx: &mut ViewContext<'_>, lane: BoardLane, count: usize) {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 26.0), egui::Sense::click());
    let painter = ui.painter();
    text::dot(
        painter,
        egui::Pos2::new(rect.left() + 4.0, rect.center().y),
        lane_color(lane),
    );
    text::line(
        painter,
        egui::Pos2::new(rect.left() + 16.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        lane.label(),
        theme::font_row_title(),
        theme::TEXT_PRIMARY,
        rect.width() - 50.0,
    );
    text::line(
        painter,
        egui::Pos2::new(rect.right() - 2.0, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        &count.to_string(),
        theme::font_meta(),
        theme::TEXT_MUTED,
        40.0,
    );
    theme::hairline(ui);
    ui.add_space(theme::SPACE_SM);
    if response.clicked() {
        // Selecting a lane heading narrows the Explorer to the matching status.
        ctx.state.status_filter = Some(lane_status(lane));
        ctx.state.go(AppView::Explorer);
    }
}

/// Moves the selected card between lanes, used by the `[` and `]` shortcuts.
pub fn move_selected_lane(ctx: &mut ViewContext<'_>, delta: i32) {
    let Some(session) = ctx.selected().cloned() else {
        return;
    };
    let lane = ctx.state.lane_of(&session).shifted(delta);
    ctx.state.set_lane(session.id, lane);
    ctx.state.notify(
        crate::state::Tone::Info,
        format!("Moved to {}", lane.label()),
    );
}

fn lane_color(lane: BoardLane) -> egui::Color32 {
    match lane {
        BoardLane::Inbox => theme::BLUE,
        BoardLane::Active => theme::GREEN,
        BoardLane::Paused => theme::ORANGE,
        BoardLane::NeedsReview => theme::PURPLE,
        BoardLane::Completed => theme::TEXT_SECONDARY,
    }
}

fn lane_status(lane: BoardLane) -> SessionStatus {
    match lane {
        BoardLane::Inbox => SessionStatus::Idle,
        BoardLane::Active => SessionStatus::Active,
        BoardLane::Paused => SessionStatus::Paused,
        BoardLane::NeedsReview => SessionStatus::NeedsReview,
        BoardLane::Completed => SessionStatus::Completed,
    }
}
