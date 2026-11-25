//! General page of the preferences window.

use super::*;

pub(super) fn draw(p: &Painter, cv: &mut Canvas, v: &View, hits: &mut Vec<(LRect, Hit)>) {
    p.text(cv, CX, 52.0, 24.0, "General", pal().fg);
    p.text(
        cv,
        CX,
        76.0,
        12.5,
        "App behavior and system integration.",
        pal().fg_dim,
    );
    section_title(p, cv, 112.0, "Updates", "Keep Quickshort current.");
    let c = card(CX, 146.0, 1);
    p.card(cv, c);
    switch_row(
        p,
        cv,
        v,
        hits,
        c,
        (Glyph::Info, "Automatically check for updates", "Once a day, in the background"),
        Sw::AutoUpd,
    );
    section_title(p, cv, 228.0, "Menu", "What shows in the menu bar icon.");
    let c = card(CX, 262.0, 1);
    p.card(cv, c);
    row_label(
        p,
        cv,
        c,
        Glyph::Folder,
        "Recent items",
        "Captures listed in the tray menu",
    );
    let mb = (c.0 + c.2 - 14.0 - 130.0, c.1 + 11.0, 130.0, 30.0);
    dropdown_btn(
        p,
        cv,
        mb,
        &format!("{} items", v.cfg.recent_max),
        v.drop_open == Some(Drop::RecentN),
        v.hover == Some(Hit::Drop(Drop::RecentN)),
    );
    hits.push((mb, Hit::Drop(Drop::RecentN)));

    section_title(p, cv, 344.0, "Reset", "Restore every setting to defaults.");
    let c = card(CX, 378.0, 1);
    p.card(cv, c);
    row_label(
        p,
        cv,
        c,
        Glyph::Gear,
        "Reset all settings",
        "Hotkey, shortcuts, accent, folders — all of it",
    );
    let rb = slot(c.1, 76.0);
    ghost(
        p,
        cv,
        rb,
        "Reset…",
        v.hover == Some(Hit::ResetAll),
        v.confirm == Some(Hit::ResetAll),
    );
    hits.push((rb, Hit::ResetAll));
}
