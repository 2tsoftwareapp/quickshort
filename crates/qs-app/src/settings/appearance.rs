//! Appearance page of the preferences window.

use super::*;

pub(super) fn draw(p: &Painter, cv: &mut Canvas, v: &View, hits: &mut Vec<(LRect, Hit)>) {
    p.text(cv, CX, 52.0, 24.0, "Appearance", pal().fg);
    p.text(cv, CX, 76.0, 12.5, "Interface and behavior.", pal().fg_dim);
    section_title(p, cv, 112.0, "Theme", "Mode and highlight color.");
    let c = card(CX, 146.0, 2);
    p.card_rows(cv, c, 2);
    row_label(
        p,
        cv,
        (c.0, c.1, c.2, ROW),
        Glyph::Drop,
        "Accent color",
        "Used for selections and highlights",
    );
    for (i, (name, rgb)) in ACCENTS.iter().enumerate() {
        let r = (
            c.0 + c.2 - 14.0 - (ACCENTS.len() - i) as f64 * 34.0,
            c.1 + 15.0,
            26.0,
            22.0,
        );
        p.swatch(
            cv,
            r,
            *rgb,
            v.cfg.accent == *name,
            v.hover == Some(Hit::Swatch(i)),
        );
        hits.push((r, Hit::Swatch(i)));
    }
    row_label(
        p,
        cv,
        (c.0, c.1 + ROW, c.2, ROW),
        Glyph::Display,
        "Mode",
        "Light or Dark appearance",
    );
    let tb = slot(c.1 + ROW, 160.0);
    let label = THEMES
        .iter()
        .find(|(k, _)| *k == v.cfg.theme)
        .map(|(_, l)| *l)
        .unwrap_or(THEMES[0].1);
    dropdown_btn(
        p,
        cv,
        tb,
        label,
        v.drop_open == Some(Drop::Theme),
        v.hover == Some(Hit::Drop(Drop::Theme)),
    );
    hits.push((tb, Hit::Drop(Drop::Theme)));
    section_title(
        p,
        cv,
        280.0,
        "After capture",
        "What happens once a capture is saved.",
    );
    let c = card(CX, 314.0, 2);
    p.card_rows(cv, c, 2);
    row_label(
        p,
        cv,
        (c.0, c.1, c.2, ROW),
        Glyph::Crop,
        "Then",
        #[cfg(target_os = "macos")]
        "Show a toast, open Quickshort Editor, or copy",
        #[cfg(not(target_os = "macos"))]
        "Thumbnail, open Quickshort Editor, or copy",
    );
    let ab = slot(c.1, 160.0);
    let label = AFTER_CAPTURE
        .iter()
        .find(|(k, _)| *k == v.cfg.after_capture)
        .map(|(_, l)| *l)
        .unwrap_or(AFTER_CAPTURE[0].1);
    dropdown_btn(
        p,
        cv,
        ab,
        label,
        v.drop_open == Some(Drop::AfterCapture),
        v.hover == Some(Hit::Drop(Drop::AfterCapture)),
    );
    hits.push((ab, Hit::Drop(Drop::AfterCapture)));
    switch_row(
        p,
        cv,
        v,
        hits,
        (c.0, c.1 + ROW, c.2, ROW),
        (Glyph::Drop, "Copy captures after saving", "Saved screenshots also go to the clipboard"),
        Sw::CopySave,
    );

    section_title(p, cv, 448.0, "Popups", "Where floating cards appear.");
    let c = card(CX, 482.0, 1);
    p.card(cv, c);
    row_label(
        p,
        cv,
        c,
        Glyph::Drop,
        "Popup position",
        "Toasts, capture cards and the REC pill",
    );
    let pb = (c.0 + c.2 - 14.0 - 140.0, c.1 + 11.0, 140.0, 30.0);
    let label = POPUP_POS
        .iter()
        .find(|(k, _)| *k == v.cfg.popup_pos)
        .map(|(_, l)| *l)
        .unwrap_or(POPUP_POS[0].1);
    dropdown_btn(
        p,
        cv,
        pb,
        label,
        v.drop_open == Some(Drop::PopupPos),
        v.hover == Some(Hit::Drop(Drop::PopupPos)),
    );
    hits.push((pb, Hit::Drop(Drop::PopupPos)));
}
