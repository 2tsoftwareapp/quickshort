//! Capture page of the preferences window.

use super::*;

pub(super) fn draw(p: &Painter, cv: &mut Canvas, v: &View, hits: &mut Vec<(LRect, Hit)>) {
    p.text(cv, CX, 52.0, 24.0, "Capture", pal().fg);
    p.text(
        cv,
        CX,
        76.0,
        12.5,
        "Configure how you capture and save screenshots.",
        pal().fg_dim,
    );
    section_title(
        p,
        cv,
        108.0,
        "Capture method",
        "Choose hotkey and capture behavior.",
    );
    let c = card(CX, 142.0, 2);
    p.card_rows(cv, c, 2);
    // row 1: region hotkey
    let r = (c.0, c.1, c.2, ROW);
    row_label(
        p,
        cv,
        r,
        Glyph::Crop,
        "Capture hotkey",
        "Select a region on screen",
    );
    let rb = slot(r.1, 76.0);
    if v.recording == Some(Rec::Global) {
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
            ("Needs a modifier, e.g. ⌘ ⌥ ⌃", pal().fg_dim)
        } else {
            ("Press shortcut", p.accent_deep)
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
        p.keycaps(
            cv,
            &v.cfg.hotkey,
            rb.0 - keycaps_w(p, &v.cfg.hotkey) - 10.0,
            r.1 + 12.0,
        );
    }
    p.ghost_button(cv, rb, "Change", v.hover == Some(Hit::Rec(Rec::Global)));
    hits.push((rb, Hit::Rec(Rec::Global)));

    // row 2: fullscreen hotkey — saves the whole screen, no selection
    let r = (c.0, c.1 + ROW, c.2, ROW);
    row_label(
        p,
        cv,
        r,
        Glyph::Display,
        "Fullscreen hotkey",
        "Save the whole screen, no selection",
    );
    let rb = slot(r.1, 76.0);
    if v.recording == Some(Rec::GlobalFull) {
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
            ("Needs a modifier, e.g. ⌘ ⌥ ⌃", pal().fg_dim)
        } else {
            ("Press shortcut", p.accent_deep)
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
        p.keycaps(
            cv,
            &v.cfg.hotkey_full,
            rb.0 - keycaps_w(p, &v.cfg.hotkey_full) - 10.0,
            r.1 + 12.0,
        );
    }
    p.ghost_button(cv, rb, "Change", v.hover == Some(Hit::Rec(Rec::GlobalFull)));
    hits.push((rb, Hit::Rec(Rec::GlobalFull)));

    section_title(
        p,
        cv,
        280.0,
        "Save location",
        "Choose where captured images are saved.",
    );
    let c = card(CX, 314.0, 1);
    p.card(cv, c);
    row_label(
        p,
        cv,
        c,
        Glyph::Folder,
        "Save folder",
        &tail(&short_path(&v.cfg.save_dir().display().to_string()), 38),
    );
    let bb = slot(c.1, 84.0);
    p.ghost_button(cv, bb, "Change…", v.hover == Some(Hit::Browse));
    hits.push((bb, Hit::Browse));

    section_title(
        p,
        cv,
        400.0,
        "Options",
        "Additional settings for capture and workflow.",
    );
    let c = card(CX, 434.0, 1);
    p.card(cv, c);
    switch_row(
        p,
        cv,
        v,
        hits,
        c,
        (Glyph::Bolt, "Launch at login", "Start Quickshort when you log in"),
        Sw::Login,
    );

    // recent captures
    p.text(cv, CX, 520.0, 14.0, "Recent captures", pal().fg);
    let va = (CX + CW - 84.0, 508.0, 84.0, 24.0);
    p.text(
        cv,
        va.0 + (va.2 - p.text_w("Open folder ›", 11.5)) / 2.0,
        521.0,
        11.5,
        "Open folder ›",
        if v.hover == Some(Hit::ViewAll) {
            pal().fg
        } else {
            p.accent_deep
        },
    );
    hits.push((va, Hit::ViewAll));
    if v.recents.is_empty() {
        p.text(
            cv,
            CX,
            566.0,
            12.0,
            "No captures yet — your latest shots will show up here.",
            pal().fg_dim,
        );
    }
    for (i, path) in v.recents.iter().enumerate() {
        let x = CX + i as f64 * (THUMB_W + 14.0);
        let tr = (x, 542.0, THUMB_W, THUMB_H);
        if v.hover == Some(Hit::Recent(i)) {
            p.fill(
                cv,
                (tr.0 - 3.0, tr.1 - 3.0, tr.2 + 6.0, tr.3 + 6.0),
                8.0,
                pal().surface_hover,
                1.0,
            );
        }
        p.thumb(cv, tr, v.thumbs.get(i).and_then(|t| t.as_ref()));
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let display_name = name.strip_prefix("quickshort-").unwrap_or(&name);
        p.text(
            cv,
            x,
            tr.1 + tr.3 + 14.0,
            10.5,
            &truncate_head(display_name, 18),
            pal().fg_dim,
        );
        p.text(
            cv,
            x,
            tr.1 + tr.3 + 28.0,
            10.0,
            &age_label(path),
            pal().fg_faint,
        );
        hits.push((
            (tr.0 - 3.0, tr.1 - 3.0, tr.2 + 6.0, tr.3 + 34.0),
            Hit::Recent(i),
        ));
    }
}
