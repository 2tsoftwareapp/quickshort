//! Shortcuts page of the preferences window.

use super::*;

/// Shortcuts page rows — label + recordable slot.
const KEY_ROWS: &[(&str, Rec)] = &[
    ("Save", Rec::Save),
    ("Done (copy + save)", Rec::Copy),
    ("Extract text (OCR)", Rec::Ocr),
    ("Share link", Rec::Share),
    ("Scrolling capture", Rec::Scroll),
    ("Screen recording", Rec::Record),
    ("Undo / Redo", Rec::Undo),
];

pub(super) fn draw(p: &Painter, cv: &mut Canvas, v: &View, hits: &mut Vec<(LRect, Hit)>) {
    p.text(cv, CX, 52.0, 24.0, "Shortcuts", pal().fg);
    p.text(
        cv,
        CX,
        76.0,
        12.5,
        "Overlay action keys — held with ⌘ or ⌃ while annotating.",
        pal().fg_dim,
    );
    let c = card(CX, 104.0, KEY_ROWS.len());
    p.card(cv, c);
    for (i, (label, rec)) in KEY_ROWS.iter().enumerate() {
        let r = (c.0, c.1 + i as f64 * ROW, c.2, ROW);
        if i > 0 {
            p.line(cv, c.0 + 14.0, r.1, c.0 + c.2 - 14.0);
        }
        row_label(p, cv, r, Glyph::Keys, label, "");
        let rb = slot(r.1, 76.0);
        if v.recording == Some(*rec) {
            let a = 0.55 + 0.45 * v.pulse.sin().abs();
            p.stroke(
                cv,
                (c.0 + 8.0, r.1 + 11.0, c.2 - 16.0, 30.0),
                7.0,
                v.cfg.accent_rgb(),
                a,
                1.0,
            );
            let (msg, col) = if v.want_mod {
                ("Hold ⌘ or ⌃ plus a key", pal().fg_dim)
            } else {
                ("Press ⌘ + key", p.accent_deep)
            };
            p.text(
                cv,
                rb.0 - p.text_w(msg, 11.5) - 12.0,
                r.1 + 29.0,
                11.5,
                msg,
                col,
            );
        } else {
            let ks = format!("⌘+{}", key_str(v.cfg, *rec).to_uppercase());
            p.keycaps(cv, &ks, rb.0 - keycaps_w(p, &ks) - 10.0, r.1 + 12.0);
        }
        p.ghost_button(cv, rb, "Change", v.hover == Some(Hit::Rec(*rec)));
        hits.push((rb, Hit::Rec(*rec)));
    }
    p.text(
        cv,
        CX,
        c.1 + c.3 + 24.0,
        11.0,
        "The Undo key also redoes with ⇧. Fixed keys: Esc cancel, Enter commit.",
        pal().fg_faint,
    );
    let rb = (CX + CW - 130.0, c.1 + c.3 + 44.0, 130.0, 30.0);
    ghost(
        p,
        cv,
        rb,
        "Reset to defaults",
        v.hover == Some(Hit::ResetKeys),
        v.confirm == Some(Hit::ResetKeys),
    );
    hits.push((rb, Hit::ResetKeys));
}
