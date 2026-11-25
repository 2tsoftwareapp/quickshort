//! OCR page of the preferences window.

use super::*;

/// apt install of missing packs — pkexec raises the polkit auth dialog
/// so the app never needs root itself. Worker thread reports via the
/// shared InstallState that about_to_wait polls.
#[cfg(target_os = "linux")]
pub(super) fn install_packs(pkgs: &[String]) -> InstallState {
    let status = std::process::Command::new("pkexec")
        .arg("apt-get")
        .arg("install")
        .arg("-y")
        .args(pkgs)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    match status {
        Ok(s) if s.success() => InstallState::Ok(format!("Installed {} pack(s)", pkgs.len())),
        Ok(s) => InstallState::Failed(format!("apt exited {s} — nothing installed")),
        Err(e) => InstallState::Failed(format!("pkexec failed: {e}")),
    }
}

#[cfg(not(target_os = "linux"))]
pub(super) fn install_packs(_: &[String]) -> InstallState {
    // unreachable — the button only renders where packs can be installed
    InstallState::Failed("pack install is Linux-only".into())
}

pub(super) fn draw(p: &Painter, cv: &mut Canvas, v: &View, hits: &mut Vec<(LRect, Hit)>) {
    p.text(cv, CX, 52.0, 24.0, "OCR", pal().fg);
    p.text(
        cv,
        CX,
        76.0,
        12.5,
        "Languages the text action (T) recognizes.",
        pal().fg_dim,
    );

    let auto = v.cfg.ocr_lang.trim() == "auto";
    section_title(
        p,
        cv,
        112.0,
        "Mode",
        if auto {
            "Every installed script is scanned — choose a language below to limit it."
        } else {
            "Only checked languages run — uncheck them all to return to auto."
        },
    );
    let c = card(CX, 146.0, 1);
    p.card(cv, c);
    let r = (c.0, c.1, c.2, ROW);
    row_label(
        p,
        cv,
        r,
        Glyph::Text,
        "Auto-detect",
        "Let the engine identify the script — recommended",
    );
    check_box(
        p,
        cv,
        r.0 + r.2 - 34.0,
        r.1 + 16.0,
        auto,
        v.hover == Some(Hit::OcrAuto),
    );
    hits.push((r, Hit::OcrAuto));

    // Media.Ocr is single-language; tesseract and Vision take lists
    let sel_note = if auto {
        "Auto-detect is on — ticking a language switches to manual."
    } else if cfg!(target_os = "windows") {
        "The first checked language wins — Media.Ocr takes one."
    } else {
        "All checked languages run together."
    };
    section_title(p, cv, 214.0, "Languages", sel_note);
    // two-column checklist, 40px rows
    const LROW: f64 = 40.0;
    let rows = qs_ocr::LANGS.len().div_ceil(2);
    let gy = 248.0;
    let gh = rows as f64 * LROW + 12.0;
    p.card(cv, (CX, gy, CW, gh));
    let col_w = (CW - 16.0) / 2.0;
    let sel = qs_ocr::lang_codes(&v.cfg.ocr_lang);
    for (i, e) in qs_ocr::LANGS.iter().enumerate() {
        let col = i / rows;
        let row = i % rows;
        let x = CX + 8.0 + col as f64 * col_w;
        let y = gy + 6.0 + row as f64 * LROW;
        let hr = (x, y, col_w, LROW);
        let on = sel.iter().any(|s| s == e.0);
        // a language the engine can't use isn't a control — except an
        // already-checked one, which must stay uncheckable
        let missing = !v.ocr_langs.iter().any(|l| l == e.0);
        let live = !missing || on;
        let hover = live && v.hover == Some(Hit::Lang(i));
        if hover {
            p.fill(cv, hr, 6.0, pal().surface_hover, 1.0);
        }
        if missing && !on {
            p.fill(cv, (x + 6.0, y + 10.0, 20.0, 20.0), 5.0, pal().tile, 0.8);
            p.stroke(
                cv,
                (x + 6.0, y + 10.0, 20.0, 20.0),
                5.0,
                pal().border,
                0.8,
                1.0,
            );
        } else {
            check_box(p, cv, x + 6.0, y + 10.0, on, hover);
        }
        p.text(
            cv,
            x + 34.0,
            y + 25.0,
            13.0,
            e.1,
            if missing { pal().fg_faint } else { pal().fg },
        );
        if missing {
            let note = "not installed";
            p.text(
                cv,
                x + col_w - 16.0 - p.text_w(note, 10.5),
                y + 25.0,
                10.5,
                note,
                pal().fg_faint,
            );
        } else {
            // code sits beside the name so the two read as one option
            p.text(
                cv,
                x + 34.0 + p.text_w(e.1, 13.0) + 8.0,
                y + 25.0,
                10.5,
                e.0,
                pal().fg_faint,
            );
        }
        if live {
            hits.push((hr, Hit::Lang(i)));
        }
    }

    // footer: install action / progress / outcome / per-OS hint
    let fy = gy + gh + 22.0;
    #[cfg(target_os = "linux")]
    {
        if v.installing {
            // pulses while the worker runs — the polkit auth dialog is
            // the user's cue that the click did something
            let a = 0.35 + 0.65 * v.pulse.sin().abs();
            p.text(
                cv,
                CX + 8.0,
                fy + 4.0,
                11.5,
                "Installing language packs… approve the system prompt",
                mix(pal().fg_faint, pal().fg_dim, a),
            );
        } else {
            let missing: Vec<&str> = qs_ocr::LANGS
                .iter()
                .map(|e| e.0)
                .filter(|c| !v.ocr_langs.iter().any(|l| l == *c))
                .collect();
            if !missing.is_empty() {
                let lb = (CX, fy - 6.0, 190.0, 30.0);
                p.ghost_button(
                    cv,
                    lb,
                    &format!("Install {} missing packs", missing.len()),
                    v.hover == Some(Hit::OcrInstall),
                );
                hits.push((lb, Hit::OcrInstall));
                p.text(
                    cv,
                    lb.0 + lb.2 + 14.0,
                    fy + 14.0,
                    11.0,
                    "or: sudo apt install tesseract-ocr-<code>",
                    pal().fg_faint,
                );
            }
            // outcome sits under the button row so the two never overlap
            let my = if missing.is_empty() {
                fy + 4.0
            } else {
                fy + 42.0
            };
            if let Some(m) = v.install_msg {
                p.text(cv, CX + 8.0, my, 11.5, m, pal().fg_dim);
            } else if missing.is_empty() {
                p.text(
                    cv,
                    CX + 8.0,
                    my,
                    11.5,
                    "All listed languages are installed.",
                    pal().fg_faint,
                );
            }
        }
    }
    #[cfg(target_os = "macos")]
    p.text(
        cv,
        CX + 8.0,
        fy + 4.0,
        11.5,
        "Recognition runs through macOS Vision — dimmed entries are unsupported.",
        pal().fg_faint,
    );
    #[cfg(target_os = "windows")]
    p.text(
        cv,
        CX + 8.0,
        fy + 4.0,
        11.5,
        "Missing packs: Windows Settings > Time & Language > Language & region.",
        pal().fg_faint,
    );
}
