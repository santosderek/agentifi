//! Status-aware composer.
//!
//! The three controls are not visually equivalent: Send is the normal path,
//! Steer is a priority interruption that only exists while Pi is working, and
//! Abort is destructive and always confirms.

use crate::{components::text, theme};
use agentifi_domain::{AgentSession, AttachmentState, SessionStatus};
use eframe::egui::{self, Align, Layout, RichText, Ui};

/// What the user asked the composer to do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ComposerAction {
    #[default]
    None,
    /// Normal next prompt.
    Send,
    /// Queue a message until the current operation is interruptible.
    FollowUp,
    /// Priority interruption while Pi is working.
    Steer,
    /// Confirmed destructive stop.
    Abort,
    /// Attach or resume before sending.
    Attach,
}

/// Draws the composer and returns the requested action.
pub fn prompt_composer(
    ui: &mut Ui,
    session: &AgentSession,
    draft: &mut String,
    confirm_abort: &mut bool,
) -> ComposerAction {
    let working = session.status.is_working();
    let finished = session.status.is_finished();
    let attached = session.attachment == AttachmentState::Attached;
    let mut action = ComposerAction::None;

    theme::panel().show(ui, |ui| {
        let hint = if !attached {
            "Attach to this session to send prompts."
        } else if working {
            "Pi is working. Steer interrupts immediately; a follow-up is queued until it can be delivered."
        } else if finished {
            "This session is read-only. Resume it to continue the work."
        } else {
            "The message is delivered to the attached Pi session."
        };
        text::line(
            ui.painter(),
            ui.cursor().left_top(),
            egui::Align2::LEFT_TOP,
            hint,
            theme::font_meta(),
            theme::TEXT_MUTED,
            ui.available_width(),
        );
        ui.add_space(18.0);

        let editable = attached && !finished;
        ui.add_enabled_ui(editable, |ui| {
            ui.add_sized(
                [ui.available_width(), 62.0],
                egui::TextEdit::multiline(draft)
                    .hint_text(if editable {
                        "Ask Pi to continue, investigate, or change direction…"
                    } else {
                        "Read-only"
                    })
                    .frame(true),
            );
        });
        ui.add_space(theme::SPACE_MD);

        ui.horizontal(|ui| {
            let has_text = !draft.trim().is_empty();
            if *confirm_abort {
                text::line(
                    ui.painter(),
                    ui.cursor().left_center() + egui::vec2(0.0, 13.0),
                    egui::Align2::LEFT_CENTER,
                    "Abort ends the current Pi execution.",
                    theme::font_meta(),
                    theme::RED,
                    260.0,
                );
                ui.add_space(268.0);
                if danger_button(ui, "Confirm abort").clicked() {
                    *confirm_abort = false;
                    action = ComposerAction::Abort;
                }
                if ui.button("Keep running").clicked() {
                    *confirm_abort = false;
                }
                return;
            }

            if !attached {
                if ui.button("Attach session").clicked() {
                    action = ComposerAction::Attach;
                }
            } else if finished {
                if ui.button("Resume session").clicked() {
                    action = ComposerAction::Attach;
                }
            } else if working {
                if ui
                    .add_enabled(has_text, egui::Button::new("Queue follow-up"))
                    .clicked()
                {
                    action = ComposerAction::FollowUp;
                }
                if ui
                    .add_enabled(has_text, egui::Button::new("Steer"))
                    .on_hover_text("Interrupt Pi and redirect it now")
                    .clicked()
                {
                    action = ComposerAction::Steer;
                }
            } else if ui
                .add_enabled(has_text, egui::Button::new("Send"))
                .clicked()
            {
                action = ComposerAction::Send;
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled(working, egui::Button::new("Abort"))
                    .on_hover_text(if working {
                        "Stop the current execution"
                    } else {
                        "Nothing is running"
                    })
                    .clicked()
                {
                    *confirm_abort = true;
                }
                ui.label(
                    RichText::new(shortcut_hint(session.status))
                        .font(theme::font_meta())
                        .color(theme::TEXT_MUTED),
                );
            });
        });
    });
    action
}

fn shortcut_hint(status: SessionStatus) -> &'static str {
    if status.is_working() {
        "Ctrl+Enter send  ·  Ctrl+S steer"
    } else {
        "Ctrl+Enter send"
    }
}

fn danger_button(ui: &mut Ui, label: &str) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(label).color(theme::RED)))
}
