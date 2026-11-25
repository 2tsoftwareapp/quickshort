//! Text editing — caret, boundary moves and the TextEdit session.

use super::*;

fn clamp_boundary(s: &str, i: usize) -> usize {
    let mut i = i.min(s.len());
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn prev_boundary(s: &str, i: usize) -> usize {
    let i = clamp_boundary(s, i);
    s[..i]
        .char_indices()
        .next_back()
        .map(|(b, _)| b)
        .unwrap_or(0)
}

fn next_boundary(s: &str, i: usize) -> usize {
    let i = clamp_boundary(s, i);
    s[i..].chars().next().map(|c| i + c.len_utf8()).unwrap_or(i)
}

impl Editor {
    // ---- text editing --------------------------------------------------------

    /// Start editing the text of `id` (caret at the end).
    pub fn begin_text_edit(&mut self, id: ObjectId) -> bool {
        self.finish_effects();
        let pre = self.snap();
        self.begin_text_edit_with(id, pre)
    }

    pub(super) fn begin_text_edit_with(&mut self, id: ObjectId, pre: Snap) -> bool {
        if self.edit.is_some() {
            self.end_text_edit();
        }
        let Some(o) = self.doc.get(id) else {
            return false;
        };
        if o.locked {
            return false;
        }
        let Some(t) = o.kind.text() else { return false };
        let len = t.len();
        self.selection = vec![id];
        self.edit = Some(TextEdit {
            id,
            caret: len,
            anchor: len,
        });
        self.edit_pre = Some(pre);
        true
    }

    pub fn end_text_edit(&mut self) {
        let Some(te) = self.edit.take() else { return };
        let pre = self.edit_pre.take();
        let empty_text = self.doc.get(te.id).is_some_and(|o| {
            matches!(o.kind, ObjectKind::Text { .. })
                && o.kind.text().is_some_and(|t| t.trim().is_empty())
        });
        if empty_text {
            self.doc.remove(te.id);
        }
        if empty_text || matches!(self.tool, Tool::Text | Tool::Callout) {
            self.selection.retain(|i| *i != te.id);
        }
        if let Some(pre) = pre {
            if pre != self.snap() {
                self.push_undo(pre);
            }
        }
    }

    pub(super) fn edit_text(&mut self, f: impl FnOnce(&mut String, &mut TextEdit)) -> bool {
        let Some(mut te) = self.edit else {
            return false;
        };
        let Some(t) = self.doc.get_mut(te.id).and_then(|o| o.kind.text_mut()) else {
            return false;
        };
        te.caret = clamp_boundary(t, te.caret);
        te.anchor = clamp_boundary(t, te.anchor);
        let before = t.clone();
        f(t, &mut te);
        let changed = *t != before;
        self.edit = Some(te);
        self.fit_callout(te.id);
        if changed {
            // Recovery/close guards must see live edits, while the session
            // still contributes only one undo entry when it ends.
            self.mark_modified();
        }
        true
    }

    /// Grow a callout so its wrapped text fits (never shrinks).
    pub(crate) fn fit_callout(&mut self, id: ObjectId) {
        let Some(o) = self.doc.get_mut(id) else {
            return;
        };
        if let ObjectKind::Callout {
            body,
            text: t,
            style,
            ..
        } = &mut o.kind
        {
            let b = body.normalized();
            let lay = text::layout(t, style, Some((b.w - 2.0 * CALLOUT_PAD).max(8.0)));
            let need = lay.height + 2.0 * CALLOUT_PAD;
            if need > b.h {
                *body = RectF::new(b.x, b.y, b.w, need);
            }
        }
    }

    pub fn text_insert(&mut self, s: &str) {
        let clean: String = s
            .chars()
            .filter(|c| *c == '\n' || !c.is_control())
            .collect();
        if clean.is_empty() {
            return;
        }
        self.edit_text(|t, te| {
            let (a, b) = te.range();
            t.replace_range(a..b, &clean);
            te.caret = a + clean.len();
            te.anchor = te.caret;
        });
    }

    pub fn text_backspace(&mut self) {
        self.edit_text(|t, te| {
            let (a, b) = te.range();
            if a != b {
                t.replace_range(a..b, "");
                te.caret = a;
            } else if a > 0 {
                let p = prev_boundary(t, a);
                t.replace_range(p..a, "");
                te.caret = p;
            }
            te.anchor = te.caret;
        });
    }

    pub fn text_delete(&mut self) {
        self.edit_text(|t, te| {
            let (a, b) = te.range();
            if a != b {
                t.replace_range(a..b, "");
                te.caret = a;
            } else if a < t.len() {
                let n = next_boundary(t, a);
                t.replace_range(a..n, "");
                te.caret = a;
            }
            te.anchor = te.caret;
        });
    }

    pub fn text_select_all(&mut self) {
        let Some(te) = self.edit else { return };
        let len = self
            .doc
            .get(te.id)
            .and_then(|o| o.kind.text())
            .map_or(0, str::len);
        self.edit = Some(TextEdit {
            caret: len,
            anchor: 0,
            ..te
        });
    }

    /// Currently selected text, for copy / cut.
    pub fn text_selection(&self) -> Option<String> {
        let te = self.edit?;
        let t = self.doc.get(te.id)?.kind.text()?;
        let (a, b) = te.range();
        let (a, b) = (clamp_boundary(t, a), clamp_boundary(t, b));
        (a != b).then(|| t[a..b].to_string())
    }

    pub fn text_move(&mut self, m: Motion, extend: bool) {
        let Some(te) = self.edit else { return };
        let (a, b) = te.range();
        let collapse = !extend && a != b;
        let new = match (collapse, m) {
            (true, Motion::Left) => a,
            (true, Motion::Right) => b,
            _ => self.motion_target(te, m),
        };
        self.edit = Some(TextEdit {
            caret: new,
            anchor: if extend { te.anchor } else { new },
            ..te
        });
    }

    pub(super) fn motion_target(&self, te: TextEdit, m: Motion) -> usize {
        let Some(o) = self.doc.get(te.id) else {
            return te.caret;
        };
        let Some(t) = o.kind.text() else {
            return te.caret;
        };
        let i = clamp_boundary(t, te.caret);
        match m {
            Motion::Left => prev_boundary(t, i),
            Motion::Right => next_boundary(t, i),
            Motion::DocStart => 0,
            Motion::DocEnd => t.len(),
            Motion::Home | Motion::End | Motion::Up | Motion::Down => {
                let Some((_, lay)) = text_block(&o.kind) else {
                    return i;
                };
                let li = lay.line_of(i);
                match m {
                    Motion::Home => lay.lines[li].start,
                    Motion::End => lay.lines[li].end,
                    Motion::Up if li == 0 => 0,
                    Motion::Up => lay.index_on_line(li - 1, lay.caret(i).0),
                    _ if li + 1 >= lay.lines.len() => t.len(),
                    _ => lay.index_on_line(li + 1, lay.caret(i).0),
                }
            }
        }
    }
}
