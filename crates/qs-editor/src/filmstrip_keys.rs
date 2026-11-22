//! Keyboard decisions for the Recent Captures tray.
//!
//! Extracted from the panel so the rules can be tested without an `EditorApp`:
//! the tray had no keyboard route at all, and Combine depends on it.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TrayInput {
    pub left: bool,
    pub right: bool,
    pub space: bool,
    pub enter: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    None,
    /// Space on the focused capture.
    ToggleMark(usize),
    /// Enter with fewer than two marks.
    Open(usize),
    /// Enter with two or more marks.
    OpenCombine,
}

/// New focus plus what Enter/Space should do.
///
/// `len` is the number of captures; `marked` is how many are already marked.
pub fn tray_keys(
    focus: Option<usize>,
    len: usize,
    marked: usize,
    input: TrayInput,
) -> (Option<usize>, TrayAction) {
    if len == 0 {
        return (None, TrayAction::None);
    }
    // The first arrow press lands on the first capture rather than skipping it.
    let had_focus = focus.is_some();
    let mut index = focus.unwrap_or(0).min(len - 1);
    if had_focus {
        if input.right {
            index = (index + 1).min(len - 1);
        } else if input.left {
            index = index.saturating_sub(1);
        }
    }
    let focus = if input.left || input.right || input.space || input.enter {
        Some(index)
    } else {
        focus
    };

    let action = if input.space {
        TrayAction::ToggleMark(index)
    } else if input.enter {
        if marked >= 2 {
            TrayAction::OpenCombine
        } else {
            TrayAction::Open(index)
        }
    } else {
        TrayAction::None
    };
    (focus, action)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(left: bool, right: bool, space: bool, enter: bool) -> TrayInput {
        TrayInput {
            left,
            right,
            space,
            enter,
        }
    }

    #[test]
    fn arrows_move_the_cursor_and_clamp_at_the_ends() {
        assert_eq!(
            tray_keys(None, 3, 0, input(false, true, false, false)),
            (Some(0), TrayAction::None),
            "the first press focuses the first capture"
        );
        assert_eq!(
            tray_keys(Some(2), 3, 0, input(false, true, false, false)).0,
            Some(2),
            "right at the end stays put"
        );
        assert_eq!(
            tray_keys(Some(0), 3, 0, input(true, false, false, false)).0,
            Some(0),
            "left at the start stays put"
        );
        assert_eq!(
            tray_keys(Some(1), 3, 0, input(true, false, false, false)).0,
            Some(0)
        );
    }

    #[test]
    fn space_marks_the_focused_capture() {
        assert_eq!(
            tray_keys(Some(1), 3, 0, input(false, false, true, false)),
            (Some(1), TrayAction::ToggleMark(1))
        );
    }

    #[test]
    fn enter_opens_combine_only_with_two_or_more_marks() {
        assert_eq!(
            tray_keys(Some(0), 3, 1, input(false, false, false, true)),
            (Some(0), TrayAction::Open(0))
        );
        assert_eq!(
            tray_keys(Some(1), 3, 2, input(false, false, false, true)),
            (Some(1), TrayAction::OpenCombine)
        );
    }

    #[test]
    fn an_empty_tray_ignores_every_key() {
        assert_eq!(
            tray_keys(None, 0, 0, input(true, true, true, true)),
            (None, TrayAction::None)
        );
    }
}
