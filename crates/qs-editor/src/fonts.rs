//! Install Be Vietnam Pro (Regular + Bold) into egui so the UI and
//! rendered text match the export output and display Vietnamese cleanly.

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    fonts.font_data.insert(
        "BeVietnamPro-Regular".to_owned(),
        FontData::from_static(qs_render::text::REGULAR_BYTES).into(),
    );
    fonts.font_data.insert(
        "BeVietnamPro-Bold".to_owned(),
        FontData::from_static(qs_render::text::BOLD_BYTES).into(),
    );

    // Make Be Vietnam Pro the primary proportional font, falling back to egui defaults.
    if let Some(prop) = fonts.families.get_mut(&FontFamily::Proportional) {
        prop.insert(0, "BeVietnamPro-Regular".to_owned());
    }
    // Also declare a named Bold family for headings / emphasis.
    fonts.families.insert(
        FontFamily::Name("Bold".into()),
        vec!["BeVietnamPro-Bold".to_owned(), "BeVietnamPro-Regular".to_owned()],
    );

    ctx.set_fonts(fonts);
}
