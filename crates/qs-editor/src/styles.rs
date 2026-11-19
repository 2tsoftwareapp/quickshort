//! Quick Styles: named presets that set every style knob of a tool at once.
//!
//! Each creative tool ships eight defaults (colours, widths, sizes, stamp
//! kinds…). The user can save the style of the current selection as a custom
//! preset; those live in `$XDG_CONFIG_HOME/quickshort/styles.json` and are
//! listed after the defaults for the same tool.
//!
//! Applying a preset goes through [`qs_edit::Editor::apply_style`], so the
//! whole preset is a single undo step and new objects keep using it.

use crate::theme;
use eframe::egui::{self, Color32, Pos2, RichText, Ui, Vec2};
use qs_doc::{
    Align, ArrowHead, BlurMode, CalloutShape, Fill, MagnifyShape, StampKind, TextStyle, PALETTE,
    PIX_BLOCK,
};
use qs_edit::{Editor, Style, Tool};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Presets kept per tool; custom ones append to the defaults.
pub const PRESETS_PER_TOOL: usize = 8;

/// One named preset. Every field mirrors [`qs_edit::Style`], so a preset can
/// be applied to any kind (knobs a kind does not have are simply ignored).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QuickStyle {
    pub name: String,
    pub rgb: u32,
    pub width: f32,
    #[serde(default)]
    pub fill: Fill,
    pub text: TextStyle,
    pub arrow_head: ArrowHead,
    pub callout_shape: CalloutShape,
    pub blur_mode: BlurMode,
    pub blur_strength: f32,
    pub magnify_zoom: f32,
    pub magnify_shape: MagnifyShape,
    pub spot_dim: f32,
    pub spot_blur: f32,
    pub stamp_kind: StampKind,
    pub eraser_width: f32,
    pub step_radius: f32,
}

impl QuickStyle {
    fn new(name: &str, rgb: u32) -> Self {
        let mut s = Self::from_style(&Style::default());
        s.name = name.to_owned();
        s.rgb = rgb;
        s
    }

    /// Capture a full style (from a selection or the tool defaults).
    pub fn from_style(st: &Style) -> Self {
        Self {
            name: String::new(),
            rgb: st.rgb,
            width: st.width,
            fill: st.fill,
            text: st.text.clone(),
            arrow_head: st.arrow_head,
            callout_shape: st.callout_shape,
            blur_mode: st.blur_mode,
            blur_strength: st.blur_strength,
            magnify_zoom: st.magnify_zoom,
            magnify_shape: st.magnify_shape,
            spot_dim: st.spot_dim,
            spot_blur: st.spot_blur,
            stamp_kind: st.stamp_kind,
            eraser_width: st.eraser_width,
            step_radius: st.step_radius,
        }
    }

    pub fn to_style(&self) -> Style {
        Style {
            rgb: self.rgb,
            width: self.width,
            fill: self.fill,
            text: self.text.clone(),
            arrow_head: self.arrow_head,
            callout_shape: self.callout_shape,
            blur_mode: self.blur_mode,
            blur_strength: self.blur_strength,
            magnify_zoom: self.magnify_zoom,
            magnify_shape: self.magnify_shape,
            spot_dim: self.spot_dim,
            spot_blur: self.spot_blur,
            stamp_kind: self.stamp_kind,
            eraser_width: self.eraser_width,
            step_radius: self.step_radius,
            ..Style::default()
        }
    }
}

/// Stable key used in the config file.
pub fn tool_key(tool: Tool) -> &'static str {
    match tool {
        Tool::Select => "select",
        Tool::Pen => "pen",
        Tool::Highlighter => "highlighter",
        Tool::Arrow => "arrow",
        Tool::Line => "line",
        Tool::Rect => "rect",
        Tool::Ellipse => "ellipse",
        Tool::Text => "text",
        Tool::Callout => "callout",
        Tool::Step => "step",
        Tool::Blur => "blur",
        Tool::Magnify => "magnify",
        Tool::Spotlight => "spotlight",
        Tool::Stamp => "stamp",
        Tool::Eraser => "eraser",
        Tool::Crop => "crop",
        Tool::Cutout => "cutout",
        Tool::SelectRegion => "region",
    }
}

pub fn tool_from_key(key: &str) -> Option<Tool> {
    [
        Tool::Pen,
        Tool::Highlighter,
        Tool::Arrow,
        Tool::Line,
        Tool::Rect,
        Tool::Ellipse,
        Tool::Text,
        Tool::Callout,
        Tool::Step,
        Tool::Blur,
        Tool::Magnify,
        Tool::Spotlight,
        Tool::Stamp,
        Tool::Eraser,
    ]
    .into_iter()
    .find(|t| tool_key(*t) == key)
}

const PALETTE_NAME: [&str; 8] = [
    "Crimson", "Amber", "Sun", "Mint", "Blue", "Violet", "White", "Ink",
];

/// Eight built-in presets for `tool` (empty for tools that take no style).
pub fn defaults_for(tool: Tool) -> Vec<QuickStyle> {
    let colours = || {
        PALETTE
            .iter()
            .zip(PALETTE_NAME)
            .map(|(&rgb, name)| QuickStyle::new(name, rgb))
            .collect::<Vec<_>>()
    };
    let mut out = match tool {
        Tool::Pen | Tool::Highlighter | Tool::Line | Tool::Arrow => colours(),
        Tool::Rect | Tool::Ellipse => colours()
            .into_iter()
            .enumerate()
            .map(|(i, mut q)| {
                // alternate outline / filled looks
                if i % 2 == 1 {
                    q.fill = Fill::Solid {
                        rgb: q.rgb,
                        alpha: 0.25,
                    };
                }
                q
            })
            .collect(),
        Tool::Text | Tool::Callout => {
            let sizes = [12.0, 14.0, 18.0, 24.0, 32.0, 48.0];
            sizes
                .iter()
                .map(|&px| {
                    let mut q = QuickStyle::new(&format!("{px:.0} px"), PALETTE[0]);
                    q.text.px = px;
                    q
                })
                .chain(["Title", "Title centred"].iter().map(|name| {
                    let mut q = QuickStyle::new(name, PALETTE[7]);
                    q.text.px = 32.0;
                    q.text.bold = true;
                    if *name == "Title centred" {
                        q.text.align = Align::Center;
                    }
                    q
                }))
                .collect()
        }
        Tool::Step => colours(),
        Tool::Blur => {
            let mut v = Vec::new();
            for (mode, label) in [(BlurMode::Pixelate, "Pixelate"), (BlurMode::Gaussian, "Blur")] {
                for (i, k) in [1.0, 2.0, 3.0, 4.0].iter().enumerate() {
                    let mut q = QuickStyle::new(&format!("{label} {}", i + 1), PALETTE[7]);
                    q.blur_mode = mode;
                    q.blur_strength = PIX_BLOCK * k;
                    v.push(q);
                }
            }
            v
        }
        Tool::Magnify => {
            let mut v = Vec::new();
            for shape in [MagnifyShape::Ellipse, MagnifyShape::Rect] {
                for zoom in [1.5f32, 2.0, 3.0, 4.0] {
                    let mut q = QuickStyle::new(&format!("{zoom:.1}× {shape:?}"), PALETTE[4]);
                    q.magnify_shape = shape;
                    q.magnify_zoom = zoom;
                    v.push(q);
                }
            }
            v
        }
        Tool::Spotlight => [0.4f32, 0.6, 0.75, 0.9]
            .iter()
            .flat_map(|&dim| {
                [0.0f32, 16.0].into_iter().map(move |feather| {
                    let name = if feather > 0.0 {
                        format!("Dim {:.0}% soft", dim * 100.0)
                    } else {
                        format!("Dim {:.0}%", dim * 100.0)
                    };
                    let mut q = QuickStyle::new(&name, PALETTE[7]);
                    q.spot_dim = dim;
                    q.spot_blur = feather;
                    q
                })
            })
            .collect(),
        Tool::Stamp => StampKind::ALL
            .iter()
            .map(|&kind| {
                let mut q = QuickStyle::new(kind.label(), PALETTE[0]);
                q.stamp_kind = kind;
                q
            })
            .collect(),
        Tool::Eraser => [4.0f32, 8.0, 12.0, 16.0, 24.0, 32.0, 48.0, 64.0]
            .iter()
            .map(|&w| {
                let mut q = QuickStyle::new(&format!("{w:.0} px"), PALETTE[7]);
                q.eraser_width = w;
                q
            })
            .collect(),
        Tool::Select | Tool::Crop | Tool::Cutout | Tool::SelectRegion => Vec::new(),
    };
    out.truncate(PRESETS_PER_TOOL);
    out
}

/// Custom presets, one list per tool key.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Library {
    #[serde(default)]
    pub custom: Vec<(String, QuickStyle)>,
}

fn config_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("quickshort"))
}

/// `$XDG_CONFIG_HOME/quickshort/styles.json`.
pub fn library_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("styles.json"))
}

impl Library {
    /// Load from disk; a missing or unreadable file is an empty library.
    pub fn load() -> Self {
        library_path().map(|p| Self::load_from(&p)).unwrap_or_default()
    }

    pub fn load_from(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Write the library, creating the config directory when needed.
    pub fn save_to(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = library_path()
            .ok_or_else(|| anyhow::anyhow!("no config directory for this user"))?;
        self.save_to(&path)
    }

    /// Custom presets for `tool`, oldest first.
    pub fn for_tool(&self, tool: Tool) -> Vec<QuickStyle> {
        let key = tool_key(tool);
        self.custom
            .iter()
            .filter(|(k, _)| k == key)
            .map(|(_, q)| q.clone())
            .collect()
    }

    /// Add or replace a preset of the same name for `tool`.
    pub fn upsert(&mut self, tool: Tool, quick: QuickStyle) {
        let key = tool_key(tool).to_owned();
        if let Some(slot) = self
            .custom
            .iter_mut()
            .find(|(k, q)| *k == key && q.name == quick.name)
        {
            slot.1 = quick;
        } else {
            self.custom.push((key, quick));
        }
    }

    pub fn remove(&mut self, tool: Tool, name: &str) {
        let key = tool_key(tool);
        self.custom
            .retain(|(k, q)| !(k == key && q.name == name));
    }
}

/// Everything the quick-style bar shows for the current tool.
pub fn presets_for(editor: &Editor, library: &Library) -> Vec<QuickStyle> {
    let tool = editor.tool();
    let mut out = defaults_for(tool);
    out.extend(library.for_tool(tool));
    out
}

/// Apply one preset: tool defaults plus the whole selection, one undo step.
pub fn apply(editor: &mut Editor, quick: &QuickStyle) -> bool {
    editor.apply_style(quick.to_style())
}

/// A readable default name for "save current style as…".
pub fn suggested_name(editor: &Editor) -> String {
    let count = editor.selection().len();
    let base = if count > 1 {
        format!("{} ×{count}", tool_label(editor.tool()))
    } else {
        tool_label(editor.tool()).to_owned()
    };
    format!("{base} custom")
}

pub fn tool_label(tool: Tool) -> &'static str {
    match tool {
        Tool::Select => "Select",
        Tool::Pen => "Pen",
        Tool::Highlighter => "Highlighter",
        Tool::Arrow => "Arrow",
        Tool::Line => "Line",
        Tool::Rect => "Rect",
        Tool::Ellipse => "Ellipse",
        Tool::Text => "Text",
        Tool::Callout => "Callout",
        Tool::Step => "Step",
        Tool::Blur => "Blur",
        Tool::Magnify => "Magnify",
        Tool::Spotlight => "Spotlight",
        Tool::Stamp => "Stamp",
        Tool::Eraser => "Eraser",
        Tool::Crop => "Crop",
        Tool::Cutout => "Cutout",
        Tool::SelectRegion => "Region",
    }
}

/// Swatch colour of a preset, for the chip.
fn swatch(quick: &QuickStyle) -> Color32 {
    Color32::from_rgb(
        (quick.rgb >> 16) as u8,
        (quick.rgb >> 8) as u8,
        quick.rgb as u8,
    )
}

/// Chips under the properties bar: one click applies the whole preset.
/// Returns true when the document changed.
pub fn render_bar(ui: &mut Ui, editor: &mut Editor, library: &mut Library) -> bool {
    let tool = editor.tool();
    let presets = presets_for(editor, library);
    let mut changed = false;
    let mut save_as: Option<QuickStyle> = None;

    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Quick Styles")
                .color(theme::text_tertiary())
                .size(11.0),
        );
        ui.add_space(4.0);
        if presets.is_empty() {
            ui.label(
                RichText::new("No presets for this tool")
                    .color(theme::text_disabled())
                    .size(11.0),
            );
        }
        egui::ScrollArea::horizontal()
            .id_salt("quick_styles")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for quick in &presets {
                        let response = chip(ui, quick);
                        if response.clicked() && apply(editor, quick) {
                            changed = true;
                        }
                        response.context_menu(|ui| {
                            if library.for_tool(tool).iter().any(|q| q.name == quick.name) {
                                if ui.button("Delete preset").clicked() {
                                    library.remove(tool, &quick.name);
                                    let _ = library.save();
                                }
                            } else {
                                ui.label(
                                    RichText::new("Built-in preset")
                                        .color(theme::text_tertiary()),
                                );
                            }
                        });
                    }
                });
            });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button("Save style")
                .on_hover_text("Save the current style as a custom preset")
                .clicked()
            {
                let mut quick = QuickStyle::from_style(&editor.current_style());
                quick.name = suggested_name(editor);
                save_as = Some(quick);
            }
        });
    });

    if let Some(quick) = save_as {
        library.upsert(tool, quick);
        let _ = library.save();
    }
    changed
}

fn chip(ui: &mut Ui, quick: &QuickStyle) -> egui::Response {
    let label_w = (quick.name.chars().count() as f32 * 6.2).clamp(18.0, 96.0);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(24.0 + label_w, 24.0), egui::Sense::click());
    let hovered = response.hovered();
    let painter = ui.painter();
    theme::paint_pill_button(painter, rect, 7.0, hovered, hovered, true, false);
    let dot = Pos2::new(rect.left() + 13.0, rect.center().y);
    painter.circle_filled(dot, 6.0, swatch(quick));
    painter.circle_stroke(
        dot,
        6.0,
        egui::Stroke::new(1.0, theme::border_subtle()),
    );
    painter.text(
        Pos2::new(dot.x + 10.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        &quick.name,
        egui::FontId::proportional(11.0),
        if hovered {
            theme::text_primary()
        } else {
            theme::text_secondary()
        },
    );
    response.on_hover_text(&quick.name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_tool() -> Vec<Tool> {
        [
            Tool::Pen,
            Tool::Highlighter,
            Tool::Arrow,
            Tool::Line,
            Tool::Rect,
            Tool::Ellipse,
            Tool::Text,
            Tool::Callout,
            Tool::Step,
            Tool::Blur,
            Tool::Magnify,
            Tool::Spotlight,
            Tool::Stamp,
            Tool::Eraser,
        ]
        .into_iter()
        .collect()
    }

    #[test]
    fn every_creative_tool_ships_eight_named_presets() {
        for tool in every_tool() {
            let presets = defaults_for(tool);
            assert_eq!(
                presets.len(),
                PRESETS_PER_TOOL,
                "{} has {} presets",
                tool_key(tool),
                presets.len()
            );
            let mut names = presets.iter().map(|p| p.name.clone()).collect::<Vec<_>>();
            names.sort();
            names.dedup();
            assert_eq!(
                names.len(),
                PRESETS_PER_TOOL,
                "{} has duplicate preset names",
                tool_key(tool)
            );
        }
        assert!(defaults_for(Tool::Select).is_empty());
        assert!(defaults_for(Tool::Crop).is_empty());
    }

    #[test]
    fn presets_round_trip_through_the_style_struct() {
        for tool in every_tool() {
            for quick in defaults_for(tool) {
                // `Style` carries no name, so compare the knobs and re-attach it
                let mut back = QuickStyle::from_style(&quick.to_style());
                back.name = quick.name.clone();
                assert_eq!(back, quick, "{} preset lost a knob", tool_key(tool));
            }
        }
    }

    #[test]
    fn custom_presets_persist_and_replace_by_name() {
        let dir = std::env::temp_dir().join(format!("qs-styles-{}", std::process::id()));
        let path = dir.join("styles.json");
        let _ = std::fs::remove_dir_all(&dir);

        let mut lib = Library::default();
        let mut mine = QuickStyle::from_style(&Style::default());
        mine.name = "Mine".to_owned();
        mine.rgb = 0x123456;
        lib.upsert(Tool::Rect, mine.clone());
        lib.save_to(&path).unwrap();

        let mut back = Library::load_from(&path);
        assert_eq!(back.for_tool(Tool::Rect), vec![mine.clone()]);
        assert!(back.for_tool(Tool::Pen).is_empty());

        mine.rgb = 0x654321;
        back.upsert(Tool::Rect, mine.clone());
        assert_eq!(back.for_tool(Tool::Rect).len(), 1, "same name replaces");
        assert_eq!(back.for_tool(Tool::Rect)[0].rgb, 0x654321);

        back.remove(Tool::Rect, "Mine");
        assert!(back.for_tool(Tool::Rect).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tool_keys_round_trip_and_ignore_junk() {
        for tool in every_tool() {
            assert_eq!(tool_from_key(tool_key(tool)), Some(tool));
        }
        assert_eq!(tool_from_key("nope"), None);
    }

    #[test]
    fn applying_a_preset_styles_the_selection_in_one_step() {
        use qs_doc::Document;
        let mut e = Editor::new(Document::new(80, 60, 1.0));
        e.set_tool(Tool::Rect);
        e.pointer_down(qs_doc::PointF::new(10.0, 10.0));
        e.pointer_move(qs_doc::PointF::new(40.0, 30.0), false);
        e.pointer_up(qs_doc::PointF::new(40.0, 30.0));

        let solid = defaults_for(Tool::Rect)
            .into_iter()
            .find(|q| q.fill != Fill::None)
            .expect("a filled default");
        assert!(apply(&mut e, &solid));
        let kind = &e.doc().objects[0].kind;
        match kind {
            qs_doc::ObjectKind::Rect { stroke, fill, .. } => {
                assert_eq!(stroke.rgb, solid.rgb);
                assert_eq!(*fill, solid.fill);
            }
            k => panic!("{k:?}"),
        }
        assert!(e.undo());
        match &e.doc().objects[0].kind {
            qs_doc::ObjectKind::Rect { fill, .. } => assert_eq!(*fill, Fill::None),
            k => panic!("{k:?}"),
        }
    }

    #[test]
    fn suggested_names_mention_the_tool_and_count() {
        use qs_doc::Document;
        let mut e = Editor::new(Document::new(20, 20, 1.0));
        e.set_tool(Tool::Arrow);
        assert_eq!(suggested_name(&e), "Arrow custom");
        assert_eq!(qs_doc::SIZES.len(), 3, "sizes stay the documented S/M/L");
    }
}
