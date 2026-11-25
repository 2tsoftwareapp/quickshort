//! About page of the preferences window.

use super::*;

pub(super) fn draw(p: &Painter, cv: &mut Canvas, v: &View, hits: &mut Vec<(LRect, Hit)>) {
    let mid = CX + CW / 2.0;
    p.text(
        cv,
        mid - p.text_w("Quickshort", 22.0) / 2.0,
        140.0,
        22.0,
        "Quickshort",
        pal().fg,
    );
    let ver = concat!("v", env!("CARGO_PKG_VERSION"));
    p.text(
        cv,
        mid - p.text_w(ver, 12.5) / 2.0,
        162.0,
        12.5,
        ver,
        pal().fg_dim,
    );
    p.text(
        cv,
        mid - p.text_w("Fast native screenshots — annotate, OCR, share.", 11.5) / 2.0,
        186.0,
        11.5,
        "Fast native screenshots — annotate, OCR, share.",
        pal().fg_faint,
    );

    let (label, w) = match v.update_ver {
        Some(vr) => (format!("Update to v{vr} — Restart"), 240.0),
        None => ("Check for Updates…".to_string(), 180.0),
    };
    let ub = (mid - w / 2.0, 220.0, w, 32.0);
    p.ghost_button(cv, ub, &label, v.hover == Some(Hit::Update));
    hits.push((ub, Hit::Update));

    // links
    let lw: f64 = LINKS
        .iter()
        .map(|(t, _)| p.text_w(t, 12.0) + 28.0)
        .sum::<f64>()
        - 28.0;
    let mut lx = mid - lw / 2.0;
    for (i, (t, _)) in LINKS.iter().enumerate() {
        let tw = p.text_w(t, 12.0);
        p.text(
            cv,
            lx,
            292.0,
            12.0,
            t,
            if v.hover == Some(Hit::Link(i)) {
                pal().fg
            } else {
                p.accent_deep
            },
        );
        if v.hover == Some(Hit::Link(i)) {
            p.seg(cv, (lx, 298.0), (lx + tw, 298.0), 1.0, pal().fg);
        }
        hits.push(((lx, 282.0, tw, 20.0), Hit::Link(i)));
        lx += tw + 28.0;
    }

    p.text(
        cv,
        mid - p.text_w("Free and open source — MIT", 10.5) / 2.0,
        570.0,
        10.5,
        "Free and open source — MIT",
        pal().fg_faint,
    );
}
