//! Find in capture: the small search bar over the recognized OCR lines.
//!
//! Kept free of recognition so it can be rendered headlessly in the UAT harness:
//! the caller passes how many lines matched and whether the engine is still
//! working, and gets back what the user asked for this frame.

use crate::theme;
use eframe::egui::{self, Vec2};
use qs_theme::{font, space};

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct FindState {
    pub open: bool,
    pub query: String,
    /// Index into the current match list; clamped by the caller.
    pub index: usize,
    /// Set when the window opens so the field takes the keyboard immediately —
    /// a find bar you have to click into is a find bar that feels broken.
    pub focus_pending: bool,
}

/// What the user asked for this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindAction {
    None,
    /// Move through the matches: `-1` previous, `1` next.
    Step(i32),
    /// Redact every matching line with a solid blackout bar.
    RedactAll,
    Close,
}

/// Draw the window and report the requested action.
pub fn render(
    ctx: &egui::Context,
    state: &mut FindState,
    matches: usize,
    scanning: bool,
) -> FindAction {
    if !state.open {
        return FindAction::None;
    }
    let mut action = FindAction::None;
    let mut open = state.open;
    // A bar pinned over the canvas, not a window: it does not move, stack or
    // take a title bar, and Esc / Close dismiss it.
    egui::Area::new(egui::Id::unique("find_bar"))
        .order(egui::Order::Foreground)
        // Clear of the toolbar and the properties bar, which are 96 px tall.
        .anchor(egui::Align2::CENTER_TOP, Vec2::new(0.0, 132.0))
        .show(ctx, |ui| {
            theme::popover_frame()
                .inner_margin(egui::Margin::symmetric(space::MD as i8, space::SM as i8))
                .show(ui, |ui| {
            ui.horizontal(|ui| {
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut state.query)
                        .hint_text("text in this capture")
                        .desired_width(220.0),
                );
                if state.focus_pending {
                    resp.request_focus();
                    state.focus_pending = false;
                }
                if resp.changed() {
                    // A new query starts from the first match.
                    state.index = 0;
                }
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    action = FindAction::Step(if ui.input(|i| i.modifiers.shift) {
                        -1
                    } else {
                        1
                    });
                }
                if crate::components::pill_button(ui, "Prev", "Previous match") {
                    action = FindAction::Step(-1);
                }
                if crate::components::pill_button(ui, "Next", "Next match") {
                    action = FindAction::Step(1);
                }
                if matches > 0
                    && crate::components::pill_button_colored(
                        ui,
                        "Redact all",
                        "Cover every matching line with a blackout bar",
                        theme::danger_red(),
                    )
                {
                    action = FindAction::RedactAll;
                }
                if crate::components::pill_button(ui, "Close", "Close find (Esc)") {
                    open = false;
                }
            });
            ui.add_space(space::SM as f32);
            // Three distinct states: still reading, nothing to search yet, and
            // a real result count. "No match" while the worker runs would be a
            // lie the user cannot tell apart from a wrong query.
            let note = if scanning {
                "Reading the text in this capture…".to_string()
            } else if state.query.trim().is_empty() {
                "Type to search the text recognized in this capture".to_string()
            } else if matches == 0 {
                format!("No match for “{}”", state.query.trim())
            } else {
                format!("{} of {} matches", state.index.min(matches - 1) + 1, matches)
            };
            ui.label(
                egui::RichText::new(note)
                    .size(font::SMALL as f32)
                    .color(theme::text_tertiary()),
            );
                });
        });
    if !open {
        return FindAction::Close;
    }
    action
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Pos2, Rect};

    fn frame(state: &mut FindState, matches: usize, scanning: bool) -> FindAction {
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 700.0))),
            ..Default::default()
        };
        let mut action = FindAction::None;
        let mut out = ctx.run_ui(raw, |_| {
            action = render(&ctx, state, matches, scanning);
        });
        out.textures_delta.clear();
        action
    }

    #[test]
    fn a_closed_state_renders_nothing_and_asks_for_nothing() {
        let mut state = FindState::default();
        assert_eq!(frame(&mut state, 0, false), FindAction::None);
    }

    #[test]
    fn an_open_window_reports_no_action_until_the_user_acts() {
        let mut state = FindState {
            open: true,
            query: "settings".into(),
            index: 1,
            focus_pending: false,
        };
        assert_eq!(frame(&mut state, 3, false), FindAction::None);
        assert_eq!(state.index, 1, "rendering must not move the selection");
    }
}
