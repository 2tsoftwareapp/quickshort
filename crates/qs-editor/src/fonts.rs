//! Install Be Vietnam Pro (Regular + Bold) into egui so the UI and
//! rendered text match the export output and display Vietnamese cleanly.

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};
use eframe::epaint::text::{FontTweak, HintingTarget, SmoothHinting};

fn crisp_tweak() -> FontTweak {
    FontTweak {
        // Snap both vertical and horizontal stems to the integer pixel grid on
        // 1x (1080p) displays so thin strokes don't smear across two grey pixels.
        hinting: Some(true),
        hinting_target: HintingTarget::Smooth(SmoothHinting {
            light: false,
            symmetric_rendering: false,
            preserve_linear_metrics: false,
        }),
        // Disable 4-way horizontal subpixel binning — sharper on 1x displays.
        subpixel_binning: Some(false),
        ..Default::default()
    }
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let tweak = crisp_tweak();

    let mut reg = FontData::from_static(qs_render::text::REGULAR_BYTES);
    reg.tweak = tweak.clone();
    fonts.font_data.insert("BeVietnamPro-Regular".to_owned(), reg.into());

    let mut bold = FontData::from_static(qs_render::text::BOLD_BYTES);
    bold.tweak = tweak;
    fonts.font_data.insert("BeVietnamPro-Bold".to_owned(), bold.into());

    // Make Be Vietnam Pro the primary proportional font, falling back to egui defaults.
    if let Some(prop) = fonts.families.get_mut(&FontFamily::Proportional) {
        prop.insert(0, "BeVietnamPro-Regular".to_owned());
    }
    // On macOS the UI reads in the system face (SF Pro), as native apps do.
    // It is loaded from the OS, never bundled: its licence is Apple-only.
    // Be Vietnam Pro stays behind it for any glyph SF lacks.
    #[cfg(target_os = "macos")]
    if let Ok(bytes) = std::fs::read("/System/Library/Fonts/SFNS.ttf") {
        fonts
            .font_data
            .insert("SF-Pro".to_owned(), FontData::from_owned(bytes).into());
        if let Some(prop) = fonts.families.get_mut(&FontFamily::Proportional) {
            prop.insert(0, "SF-Pro".to_owned());
        }
    }
    // Also declare a named Bold family for headings / emphasis.
    fonts.families.insert(
        FontFamily::Name("Bold".into()),
        vec!["BeVietnamPro-Bold".to_owned(), "BeVietnamPro-Regular".to_owned()],
    );

    ctx.set_fonts(fonts);
}
