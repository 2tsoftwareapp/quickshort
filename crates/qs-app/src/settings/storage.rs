//! Storage page of the preferences window.

use super::*;

pub(super) fn draw(p: &Painter, cv: &mut Canvas, v: &View, hits: &mut Vec<(LRect, Hit)>) {
    p.text(cv, CX, 52.0, 24.0, "Storage", pal().fg);
    p.text(cv, CX, 76.0, 12.5, "Manage captured files.", pal().fg_dim);
    section_title(p, cv, 112.0, "Location", "Where screenshots live.");
    let c = card(CX, 146.0, 2);
    p.card(cv, c);
    row_label(
        p,
        cv,
        (c.0, c.1, c.2, ROW),
        Glyph::Folder,
        &short_path(&v.cfg.save_dir().display().to_string()),
        &format!("{} captures · {}", v.stats.0, human_bytes(v.stats.1)),
    );
    let bb = slot(c.1, 110.0);
    p.ghost_button(cv, bb, "Open in Finder", v.hover == Some(Hit::OpenDir));
    hits.push((bb, Hit::OpenDir));

    // second row: volume headroom — a segmented disk chart made a few
    // MB of captures indistinguishable from rounding noise
    p.line(cv, c.0 + 14.0, c.1 + ROW, c.0 + c.2 - 14.0);
    let r = (c.0, c.1 + ROW, c.2, ROW);
    row_label(
        p,
        cv,
        r,
        Glyph::Folder,
        "Free space",
        "On the disk holding your captures",
    );
    if let Some((total, free)) = v.disk {
        let txt = format!("{} free of {}", human_bytes(free), human_bytes(total));
        p.text(
            cv,
            r.0 + r.2 - 14.0 - p.text_w(&txt, 10.5),
            r.1 + 32.0,
            10.5,
            &txt,
            pal().fg_dim,
        );
    }

    section_title(
        p,
        cv,
        280.0,
        "Retention",
        "Automatically delete old captures.",
    );
    let c = card(CX, 314.0, 2);
    p.card(cv, c);
    let r = (c.0, c.1, c.2, ROW);
    row_label(
        p,
        cv,
        r,
        Glyph::Gear,
        "Keep captures",
        if v.cfg.keep_days == 0 {
            "Files stay until you remove them"
        } else {
            "Older files are removed automatically"
        },
    );
    let kb = (r.0 + r.2 - 14.0 - 150.0, r.1 + 11.0, 150.0, 30.0);
    let label = match v.cfg.keep_days {
        7 => "7 days",
        30 => "30 days",
        _ => "Forever",
    };
    dropdown_btn(
        p,
        cv,
        kb,
        label,
        v.drop_open == Some(Drop::KeepDays),
        v.hover == Some(Hit::Drop(Drop::KeepDays)),
    );
    hits.push((kb, Hit::Drop(Drop::KeepDays)));
    p.line(cv, c.0 + 14.0, c.1 + ROW, c.0 + c.2 - 14.0);
    let r = (c.0, c.1 + ROW, c.2, ROW);
    row_label(
        p,
        cv,
        r,
        Glyph::Folder,
        "Clean up now",
        if v.cfg.keep_days == 0 {
            "Choose a retention period first"
        } else {
            "Apply the retention policy immediately"
        },
    );
    let rb = slot(r.1, 110.0);
    if v.cfg.keep_days == 0 {
        // no policy, no action — a live button that deletes nothing is
        // worse than a visibly disabled one
        p.ghost_disabled(cv, rb, "Clean now");
    } else {
        ghost(
            p,
            cv,
            rb,
            "Clean now",
            v.hover == Some(Hit::Clean),
            v.confirm == Some(Hit::Clean),
        );
        hits.push((rb, Hit::Clean));
    }
    if let Some(m) = v.clean_msg {
        p.text(cv, CX + 8.0, c.1 + c.3 + 26.0, 11.5, m, pal().fg_dim);
    }
}

fn human_bytes(b: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let (mut v, mut u) = (b as f64, 0);
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{b} B")
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}
