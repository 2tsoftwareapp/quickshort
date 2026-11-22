//! Library view: browse, search, tag and prune the capture folder.
//!
//! Scanning and thumbnail decoding happen on worker threads; this module only
//! paints state it already owns, so no frame ever waits on the filesystem.

use crate::components;
use crate::theme;
use eframe::egui::{
    self, Align2, Color32, ColorImage, CornerRadius, FontId, Pos2, Rect, TextureHandle,
    TextureOptions, Vec2,
};
use qs_library::{
    index, query, scan, DateFilter, EntryKind, FilterState, IndexState, LibraryEntry, LibraryIndex,
    Sort,
};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};

/// Decoded thumbnail box. Matches the filmstrip so the two surfaces agree.
const THUMB_W: u32 = 132;
const THUMB_H: u32 = 84;
/// Tall enough to hold the 84 px thumbnail with breathing room; a shorter
/// row let the thumbnail bleed into the next one.
const ROW_H: f32 = 92.0;
/// Fixed star column, then a gap, then the thumbnail: keeps every row aligned.
const STAR_COL: f32 = 34.0;
const GAP: f32 = 8.0;
/// Margin the dialog keeps from the window edge.
const MARGIN: f32 = 24.0;
/// Thumbnails requested but not yet returned; keeps the worker from being
/// flooded when a 10k-entry folder is filtered to everything.
const MAX_INFLIGHT: usize = 12;

struct ScanOutcome {
    entries: Vec<LibraryEntry>,
    index: Option<LibraryIndex>,
    notice: Option<String>,
    read_only: bool,
    error: Option<String>,
}

fn scan_library(dir: &Path) -> ScanOutcome {
    let (loaded, notice, read_only) = match index::load(dir) {
        IndexState::Ready(idx) => (Some(idx), None, false),
        IndexState::Rebuild { notice } => (None, (!notice.is_empty()).then_some(notice), false),
        IndexState::TooNew { notice, .. } => (None, Some(notice), true),
    };
    match scan::read_dir(dir) {
        Ok(scanned) => {
            let merged = index::merge(loaded.as_ref(), scanned);
            ScanOutcome {
                entries: merged.entries.values().cloned().collect(),
                index: Some(merged),
                notice,
                read_only,
                error: None,
            }
        }
        Err(e) => ScanOutcome {
            entries: Vec::new(),
            index: None,
            notice,
            read_only,
            error: Some(format!("{e:#}")),
        },
    }
}

fn thumb_image(path: &Path) -> Option<ColorImage> {
    let rgba = if scan::kind_of(path) == Some(EntryKind::Document) {
        qs_doc::qsdoc::read_thumb(path).ok()?
    } else {
        image::open(path).ok()?.to_rgba8()
    };
    // `thumbnail` fits inside the box without distorting the aspect ratio.
    let small = image::imageops::thumbnail(&rgba, THUMB_W, THUMB_H);
    Some(ColorImage::from_rgba_unmultiplied(
        [small.width() as usize, small.height() as usize],
        small.as_raw(),
    ))
}

/// Lazily-started worker that decodes thumbnails off the UI thread.
struct Thumbs {
    jobs: Sender<PathBuf>,
    done: Receiver<(PathBuf, Option<ColorImage>)>,
    inflight: usize,
    /// `None` records "requested and failed" so a broken file is not retried
    /// every frame.
    map: HashMap<PathBuf, Option<TextureHandle>>,
}

impl Thumbs {
    fn start() -> Self {
        let (job_tx, job_rx) = mpsc::channel::<PathBuf>();
        let (res_tx, res_rx) = mpsc::channel();
        std::thread::spawn(move || {
            for path in job_rx {
                let image = thumb_image(&path);
                if res_tx.send((path, image)).is_err() {
                    break;
                }
            }
        });
        Self {
            jobs: job_tx,
            done: res_rx,
            inflight: 0,
            map: HashMap::new(),
        }
    }
}

/// What the view wants the editor to do after this frame.
#[derive(Debug, Clone, PartialEq)]
pub enum LibraryAction {
    Open(PathBuf),
    Reveal(PathBuf),
}

pub struct Library {
    pub open: bool,
    dir: PathBuf,
    index: Option<LibraryIndex>,
    entries: Vec<LibraryEntry>,
    filter: FilterState,
    notice: Option<String>,
    read_only: bool,
    error: Option<String>,
    scanning: bool,
    scan_rx: Option<(PathBuf, Receiver<ScanOutcome>)>,
    thumbs: Thumbs,
    selected: BTreeSet<String>,
    anchor: Option<String>,
    tag_draft: String,
    confirm_delete: bool,
    /// Last persistence failure, shown until the next successful write.
    write_error: Option<String>,
}

impl Default for Library {
    fn default() -> Self {
        Self {
            open: false,
            dir: PathBuf::new(),
            index: None,
            entries: Vec::new(),
            filter: FilterState::default(),
            notice: None,
            read_only: false,
            error: None,
            scanning: false,
            scan_rx: None,
            thumbs: Thumbs::start(),
            selected: BTreeSet::new(),
            anchor: None,
            tag_draft: String::new(),
            confirm_delete: false,
            write_error: None,
        }
    }
}

impl Library {
    pub fn toggle(&mut self) {
        self.open = !self.open;
        if self.open && self.entries.is_empty() && !self.scanning {
            self.refresh();
        }
    }

    /// Point the view at a capture folder, rescanning when it changes.
    pub fn set_dir(&mut self, dir: PathBuf) {
        if self.dir == dir {
            return;
        }
        self.dir = dir;
        self.entries.clear();
        self.index = None;
        self.thumbs.map.clear();
        self.selected.clear();
        self.anchor = None;
        if self.open {
            self.refresh();
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Entries currently indexed (before the search/type/date filters).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn is_scanning(&self) -> bool {
        self.scanning
    }

    pub fn refresh(&mut self) {
        let dir = self.dir.clone();
        if dir.as_os_str().is_empty() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let scan_dir = dir.clone();
        std::thread::spawn(move || {
            let _ = tx.send(scan_library(&scan_dir));
        });
        self.scan_rx = Some((dir, rx));
        self.scanning = true;
    }

    fn poll_scan(&mut self) {
        let Some((_, rx)) = &self.scan_rx else {
            return;
        };
        match rx.try_recv() {
            Ok(outcome) => {
                self.scan_rx = None;
                self.scanning = false;
                self.notice = outcome.notice;
                self.read_only = outcome.read_only;
                self.error = outcome.error;
                self.index = outcome.index;
                self.entries = outcome.entries;
                let present: BTreeSet<String> =
                    self.entries.iter().map(|e| e.file_name.clone()).collect();
                self.selected.retain(|n| present.contains(n));
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.scan_rx = None;
                self.scanning = false;
                self.error = Some("Capture folder scan stopped unexpectedly.".to_string());
            }
        }
    }

    fn poll_thumbs(&mut self, ctx: &egui::Context) {
        loop {
            let received = match self.thumbs.done.try_recv() {
                Ok(v) => v,
                Err(_) => break,
            };
            self.thumbs.inflight = self.thumbs.inflight.saturating_sub(1);
            let (path, image) = received;
            match image {
                Some(image) => {
                    let name = format!("lib_thumb:{}", path.display());
                    let texture = ctx.load_texture(name, image, TextureOptions::LINEAR);
                    self.thumbs.map.insert(path, Some(texture));
                }
                None => {
                    self.thumbs.map.insert(path, None);
                }
            }
        }
    }

    fn request_thumb(&mut self, path: &Path) {
        if self.thumbs.inflight >= MAX_INFLIGHT || self.thumbs.map.contains_key(path) {
            return;
        }
        if self.thumbs.jobs.send(path.to_path_buf()).is_ok() {
            self.thumbs.inflight += 1;
        }
    }

    /// Write annotations back. Refused when the on-disk index belongs to a newer
    /// build, so this build never destroys annotations it cannot read.
    fn persist(&mut self) -> bool {
        if self.read_only {
            self.write_error = Some(
                "Library index was written by a newer quickshort; changes are not saved."
                    .to_string(),
            );
            return false;
        }
        let Some(index) = &self.index else {
            self.write_error = Some("No library index to save yet.".to_string());
            return false;
        };
        match index::save(&self.dir, index) {
            Ok(()) => {
                self.write_error = None;
                true
            }
            Err(e) => {
                self.write_error = Some(format!("Could not save library index: {e:#}"));
                false
            }
        }
    }

    /// The mutable entry the footer edits, when exactly one row is selected.
    fn sole_selection(&self) -> Option<&str> {
        if self.selected.len() != 1 {
            return None;
        }
        self.selected.iter().next().map(String::as_str)
    }

    fn entry_mut(&mut self, name: &str) -> Option<&mut LibraryEntry> {
        let entry = self.entries.iter_mut().find(|e| e.file_name == name)?;
        // Keep the persisted index in step with the view.
        if let Some(index) = &mut self.index {
            index.entries.insert(name.to_string(), entry.clone());
        }
        Some(entry)
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<LibraryAction> {
        if !self.open {
            return None;
        }
        self.poll_scan();
        self.poll_thumbs(ctx);

        let mut open = self.open;
        let mut action = None;
        // Leave a real margin: at 720x600 the old default filled the window and
        // covered the status bar.
        let avail = ctx.content_rect().size();
        let size = Vec2::new(
            (avail.x - 2.0 * MARGIN).clamp(420.0, 860.0),
            (avail.y - 2.0 * MARGIN).clamp(320.0, 620.0),
        );
        egui::Window::new("Library")
            .frame(theme::modal_frame())
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(size)
            .min_size(Vec2::new(420.0, 320.0))
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                action = self.body(ui);
            });
        // `body` can close the view itself (Escape); copying `open` back
        // unconditionally used to resurrect it.
        self.open = open && self.open;
        action
    }

    fn body(&mut self, ui: &mut egui::Ui) -> Option<LibraryAction> {
        let action = None;
        self.toolbar(ui);
        ui.add_space(qs_theme::space::SM as f32);

        if let Some(err) = &self.error {
            self.banner(ui, err, theme::danger_red());
        }
        if let Some(note) = &self.notice {
            self.banner(ui, note, theme::amber_orange());
        }
        if let Some(err) = &self.write_error {
            self.banner(ui, err, theme::danger_red());
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let total = self.entries.len();
        let names: Vec<String> = self
            .filter
            .apply(self.entries.iter(), now)
            .into_iter()
            .map(|e| e.file_name.clone())
            .collect();

        // Keyboard route through the library. Without it the view is
        // mouse-only, which is both an accessibility gap and the reason this
        // feature could not be exercised on a machine where synthetic clicks
        // are blocked. Typing in the search or tag field takes precedence.
        let mut action = action;
        if ui.memory(|m| m.focused().is_none()) {
            let (down, up, enter, esc, delete) = ui.input(|i| {
                (
                    i.key_pressed(egui::Key::ArrowDown),
                    i.key_pressed(egui::Key::ArrowUp),
                    i.key_pressed(egui::Key::Enter),
                    i.key_pressed(egui::Key::Escape),
                    i.modifiers.command && i.key_pressed(egui::Key::Backspace),
                )
            });
            if esc {
                self.open = false;
            }
            if (down || up) && !names.is_empty() {
                let current = self
                    .selected
                    .iter()
                    .next()
                    .and_then(|n| names.iter().position(|x| x == n));
                let next = match current {
                    Some(i) if down => (i + 1).min(names.len() - 1),
                    Some(i) => i.saturating_sub(1),
                    None => 0,
                };
                self.selected.clear();
                self.selected.insert(names[next].clone());
                self.anchor = Some(names[next].clone());
            }
            if enter && self.selected.len() == 1 {
                if let Some(name) = self.selected.iter().next() {
                    if let Some(entry) = self.entries.iter().find(|e| &e.file_name == name) {
                        action = Some(LibraryAction::Open(entry.path(&self.dir)));
                    }
                }
            }
            if delete && !self.read_only && !self.selected.is_empty() {
                self.confirm_delete = true;
            }
        }

        ui.horizontal(|ui| {
            let summary = if self.scanning {
                "Scanning…".to_string()
            } else if self.filter.search.trim().is_empty()
                && self.filter.kind.is_none()
                && self.filter.tag.is_none()
                && !self.filter.favorites_only
                && self.filter.date == DateFilter::All
            {
                format!("{total} captures")
            } else {
                format!("{} of {total} captures", names.len())
            };
            ui.label(
                egui::RichText::new(summary)
                    .size(qs_theme::font::SMALL as f32)
                    .color(theme::text_tertiary()),
            );
            if let Some(dir) = self.dir.to_str() {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(dir)
                            .size(qs_theme::font::CAPTION as f32)
                            .color(theme::text_tertiary()),
                    );
                });
            }
        });
        ui.add_space(qs_theme::space::XS as f32);
        if names.is_empty() {
            components::empty_state(
                ui,
                if self.scanning {
                    "Scanning the capture folder…"
                } else if total == 0 {
                    "No captures yet"
                } else {
                    "Nothing matches these filters"
                },
                "New captures appear here automatically.",
                None,
            );
        } else {
            let mut clicked: Option<(String, bool, bool)> = None;
            let mut open_row: Option<PathBuf> = None;
            let mut star_hit: Option<String> = None;
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height((ui.available_height() - 56.0).max(80.0))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    for name in &names {
                        let Some(entry) = self.entries.iter().find(|e| &e.file_name == name) else {
                            continue;
                        };
                        let path = entry.path(&self.dir);
                        let selected = self.selected.contains(name);
                        let thumb = self.thumbs.map.get(&path).and_then(|t| t.as_ref());
                        let hit = self.row(ui, entry, selected, thumb);
                        match hit {
                            RowHit::None => {}
                            RowHit::Click { cmd, shift } => {
                                clicked = Some((name.clone(), cmd, shift))
                            }
                            RowHit::Star => star_hit = Some(name.clone()),
                            RowHit::Open => open_row = Some(path.clone()),
                        }
                        if thumb.is_none() && !self.thumbs.map.contains_key(&path) {
                            self.request_thumb(&path);
                        }
                    }
                });
            if let Some(name) = star_hit {
                let mut changed = false;
                if let Some(entry) = self.entry_mut(&name) {
                    entry.favorite = !entry.favorite;
                    changed = true;
                }
                if changed {
                    self.persist();
                }
            }
            if let Some((name, cmd, shift)) = clicked {
                self.select(&name, cmd, shift, &names);
            }
            if let Some(path) = open_row {
                action = Some(LibraryAction::Open(path));
            }
        }

        // An empty footer is just a hairline eating 90 px of list space.
        if !self.selected.is_empty() {
            ui.separator();
            if let Some(act) = self.footer(ui) {
                action = Some(act);
            }
        }
        self.delete_dialog(ui.ctx());
        action
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        // Two deliberate rows instead of one wrapping row: a separator inside a
        // wrapped row leaves a stray `|` at the start of the next line.
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Search")
                    .size(qs_theme::font::SMALL as f32)
                    .color(theme::text_secondary()),
            );
            // The field takes the slack and Refresh sits on the far edge, so
            // the row has no dead gap at any width.
            let field_w = (ui.available_width() - 96.0).max(150.0);
            ui.add(
                egui::TextEdit::singleline(&mut self.filter.search)
                    .hint_text("name or tag")
                    .desired_width(field_w),
            );
            if self.scanning {
                ui.label(
                    egui::RichText::new("Scanning…")
                        .size(qs_theme::font::SMALL as f32)
                        .color(theme::text_tertiary()),
                );
            } else if components::pill_button(ui, "Refresh", "Rescan the folder") {
                self.refresh();
            }
        });

        ui.add_space(qs_theme::space::MD as f32);
        ui.horizontal(|ui| {
            let mut kind = self.filter.kind;
            if components::status_pill(ui, "All", kind.is_none(), "Every file type") {
                kind = None;
            }
            if components::status_pill(ui, "Images", kind == Some(EntryKind::Image), "") {
                kind = Some(EntryKind::Image);
            }
            if components::status_pill(ui, "Docs", kind == Some(EntryKind::Document), "") {
                kind = Some(EntryKind::Document);
            }
            self.filter.kind = kind;

            ui.add_space(qs_theme::space::LG as f32);
            let mut date = self.filter.date;
            for option in [
                DateFilter::All,
                DateFilter::Today,
                DateFilter::Last7Days,
                DateFilter::Last30Days,
            ] {
                if components::status_pill(ui, option.label(), date == option, "") {
                    date = option;
                }
            }
            self.filter.date = date;
        });

        ui.add_space(qs_theme::space::SM as f32);
        ui.horizontal(|ui| {
            let mut sort = self.filter.sort;
            for option in [Sort::Newest, Sort::Oldest, Sort::Name] {
                if components::status_pill(ui, option.label(), sort == option, "") {
                    sort = option;
                }
            }
            self.filter.sort = sort;

            ui.add_space(qs_theme::space::XL as f32);
            if components::status_pill(
                ui,
                "Favourites",
                self.filter.favorites_only,
                "Only favourites",
            ) {
                self.filter.favorites_only = !self.filter.favorites_only;
            }
        });

        let tags = index::all_tags(self.entries.iter());
        if !tags.is_empty() {
            ui.add_space(qs_theme::space::SM as f32);
            ui.horizontal_wrapped(|ui| {
                if components::status_pill(ui, "All tags", self.filter.tag.is_none(), "") {
                    self.filter.tag = None;
                }
                for tag in tags {
                    let active = self.filter.tag.as_deref() == Some(tag.as_str());
                    if components::status_pill(ui, &tag, active, "") {
                        self.filter.tag = if active { None } else { Some(tag) };
                    }
                }
            });
        }
    }

    fn banner(&self, ui: &mut egui::Ui, text: &str, color: Color32) {
        ui.label(
            egui::RichText::new(text)
                .size(qs_theme::font::SMALL as f32)
                .color(color),
        );
    }

    fn select(&mut self, name: &str, cmd: bool, shift: bool, order: &[String]) {
        if shift {
            let from = self
                .anchor
                .as_ref()
                .and_then(|a| order.iter().position(|n| n == a))
                .unwrap_or(0);
            let to = order.iter().position(|n| n == name).unwrap_or(from);
            let (lo, hi) = if from <= to { (from, to) } else { (to, from) };
            for n in &order[lo..=hi] {
                self.selected.insert(n.clone());
            }
            return;
        }
        if cmd {
            if !self.selected.remove(name) {
                self.selected.insert(name.to_string());
            }
            self.anchor = Some(name.to_string());
            return;
        }
        self.selected.clear();
        self.selected.insert(name.to_string());
        self.anchor = Some(name.to_string());
    }

    fn row(
        &self,
        ui: &mut egui::Ui,
        entry: &LibraryEntry,
        selected: bool,
        thumb: Option<&TextureHandle>,
    ) -> RowHit {
        let width = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, ROW_H), egui::Sense::hover());
        let resp = ui.interact(
            rect,
            ui.make_persistent_id(("lib_row", &entry.file_name)),
            egui::Sense::click(),
        );

        let painter = ui.painter();
        // A solid accent fill made the row name and meta line unreadable; the
        // selected state is a light wash plus an accent edge instead.
        let fill = if selected {
            // 36/255 disappears on the dark card fill.
            theme::accent_alpha(if qs_theme::pal().dark { 72 } else { 36 })
        } else if resp.hovered() {
            theme::bg_pill_hover()
        } else {
            theme::bg_card()
        };
        painter.rect_filled(rect, CornerRadius::same(8), fill);
        painter.rect_stroke(
            rect,
            CornerRadius::same(8),
            egui::Stroke::new(
                if selected { 1.5 } else { 1.0 },
                if selected {
                    theme::accent_blue()
                } else {
                    theme::border_subtle()
                },
            ),
            egui::StrokeKind::Inside,
        );

        // Favourite marker, on its own fixed column so the thumbnails line up.
        let star_center = Pos2::new(rect.left() + 20.0, rect.center().y);
        let star_color = if entry.favorite {
            theme::amber_orange()
        } else {
            // Tertiary was nearly invisible on the card fill.
            theme::text_secondary()
        };
        paint_star(painter, star_center, 6.0, star_color, entry.favorite);

        // Thumbnail.
        let thumb_rect = Rect::from_min_size(
            Pos2::new(
                rect.left() + STAR_COL + GAP,
                rect.center().y - THUMB_H as f32 / 2.0,
            ),
            Vec2::new(THUMB_W as f32, THUMB_H as f32),
        );
        match thumb {
            Some(texture) => {
                let size = texture.size_vec2();
                let scale = (thumb_rect.width() / size.x).min(thumb_rect.height() / size.y);
                let fitted = Rect::from_center_size(thumb_rect.center(), size * scale);
                // Keep the decoded pixels inside the row box.
                painter.rect_filled(thumb_rect, CornerRadius::same(4), theme::bg_status());
                painter.image(
                    texture.id(),
                    fitted,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
                painter.rect_stroke(
                    thumb_rect,
                    CornerRadius::same(4),
                    egui::Stroke::new(1.0, theme::border_subtle()),
                    egui::StrokeKind::Inside,
                );
            }
            None => {
                painter.rect_filled(thumb_rect, CornerRadius::same(4), theme::bg_status());
                painter.text(
                    thumb_rect.center(),
                    Align2::CENTER_CENTER,
                    entry.kind.label(),
                    FontId::proportional(10.0),
                    theme::text_tertiary(),
                );
            }
        }

        let text_left = thumb_rect.right() + 12.0;
        let right_edge = rect.right() - 76.0;
        let clipped = painter.with_clip_rect(Rect::from_min_max(
            Pos2::new(text_left, rect.top()),
            Pos2::new(right_edge, rect.bottom()),
        ));
        clipped.text(
            Pos2::new(text_left, rect.center().y - 9.0),
            Align2::LEFT_CENTER,
            &entry.file_name,
            FontId::proportional(qs_theme::font::BODY as f32),
            theme::text_primary(),
        );
        let mut meta = vec![query::format_date(entry.modified_unix)];
        if entry.width > 0 && entry.height > 0 {
            meta.push(format!("{}×{}", entry.width, entry.height));
        }
        meta.push(entry.mode.label().to_string());
        if let Some(app) = &entry.source_app {
            meta.push(app.clone());
        }
        if !entry.tags.is_empty() {
            meta.push(entry.tags.join(", "));
        }
        clipped.text(
            Pos2::new(text_left, rect.center().y + 10.0),
            Align2::LEFT_CENTER,
            meta.join(" · "),
            FontId::proportional(qs_theme::font::SMALL as f32),
            theme::text_tertiary(),
        );

        // Open is a pill like every other affordance in the app, not bare text.
        let open_rect = Rect::from_center_size(
            Pos2::new(rect.right() - 16.0 - 30.0, rect.center().y),
            Vec2::new(60.0, 26.0),
        );
        let open_hovered = ui.rect_contains_pointer(open_rect);
        painter.rect_filled(
            open_rect,
            CornerRadius::same(7),
            if open_hovered {
                theme::bg_btn_secondary_hover_top()
            } else {
                theme::bg_btn_secondary_top()
            },
        );
        painter.rect_stroke(
            open_rect,
            CornerRadius::same(7),
            egui::Stroke::new(1.0, theme::border_subtle()),
            egui::StrokeKind::Inside,
        );
        painter.text(
            open_rect.center(),
            Align2::CENTER_CENTER,
            "Open",
            FontId::proportional(qs_theme::font::SMALL as f32),
            theme::text_primary(),
        );
        let open = ui
            .put(open_rect, egui::Button::new("").frame(false))
            .on_hover_text("Open in the editor");

        // Clicking the star area toggles the favourite; everything else selects.
        let star_rect = Rect::from_center_size(star_center, Vec2::splat(24.0));
        let star_click =
            resp.clicked() && star_rect.contains(resp.interact_pointer_pos().unwrap_or(Pos2::ZERO));
        if open.clicked() || resp.double_clicked() {
            return RowHit::Open;
        }
        if star_click {
            return RowHit::Star;
        }
        if resp.clicked() {
            let (cmd, shift) = ui.input(|i| (i.modifiers.command, i.modifiers.shift));
            return RowHit::Click { cmd, shift };
        }
        RowHit::None
    }

    fn footer(&mut self, ui: &mut egui::Ui) -> Option<LibraryAction> {
        let mut action = None;
        let sole = self.sole_selection().map(str::to_string);
        // Snapshot the editable entry so the widgets below can borrow `self`.
        let sole_tags: Vec<String> = sole
            .as_ref()
            .and_then(|n| self.entries.iter().find(|e| &e.file_name == n))
            .map(|e| e.tags.clone())
            .unwrap_or_default();
        let sole_path: Option<PathBuf> = sole
            .as_ref()
            .and_then(|n| self.entries.iter().find(|e| &e.file_name == n))
            .map(|e| e.path(&self.dir));
        let count = self.selected.len();
        let read_only = self.read_only;
        let mut add_tag = false;
        let mut drop_tag: Option<String> = None;

        ui.horizontal_wrapped(|ui| {
            if sole.is_some() {
                ui.label(
                    egui::RichText::new("Tags")
                        .size(qs_theme::font::SMALL as f32)
                        .color(theme::text_secondary()),
                );
                for tag in &sole_tags {
                    if ui
                        .add(egui::Button::new(format!("{tag} ×")).frame(false))
                        .on_hover_text("Remove tag")
                        .clicked()
                    {
                        drop_tag = Some(tag.clone());
                    }
                }
                ui.add_enabled(
                    !read_only,
                    egui::TextEdit::singleline(&mut self.tag_draft)
                        .hint_text("add tag")
                        .desired_width(96.0),
                );
                add_tag = (!read_only && components::pill_button(ui, "Add", "Attach this tag"))
                    || (ui.input(|i| i.key_pressed(egui::Key::Enter))
                        && !self.tag_draft.trim().is_empty());
            } else if count > 1 {
                ui.label(
                    egui::RichText::new(format!("{count} selected"))
                        .size(qs_theme::font::SMALL as f32)
                        .color(theme::text_secondary()),
                );
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if count > 0 {
                    if !read_only
                        && components::pill_button_colored(
                            ui,
                            &format!("Move {count} to Trash"),
                            "Files go to the system trash",
                            theme::danger_red(),
                        )
                    {
                        self.confirm_delete = true;
                    }
                    if count == 1
                        && components::pill_button(ui, "Reveal", "Show in the file manager")
                    {
                        action = sole_path.clone().map(LibraryAction::Reveal);
                    }
                }
            });
        });

        if add_tag {
            if let Some(target) = sole.clone() {
                let typed = self.tag_draft.trim().to_string();
                let mut changed = false;
                if let Some(e) = self.entry_mut(&target) {
                    changed = e.add_tag(&typed);
                }
                if changed {
                    self.persist();
                }
                self.tag_draft.clear();
            }
        }
        if let Some(tag) = drop_tag {
            if let Some(target) = sole.clone() {
                let mut changed = false;
                if let Some(e) = self.entry_mut(&target) {
                    changed = e.remove_tag(&tag);
                }
                if changed {
                    self.persist();
                }
            }
        }
        action
    }

    fn delete_dialog(&mut self, ctx: &egui::Context) {
        if !self.confirm_delete {
            return;
        }
        let paths: Vec<PathBuf> = self
            .selected
            .iter()
            .map(|name| self.dir.join(name))
            .collect();
        let mut open = true;
        let mut decision = None;
        egui::Window::new("Move to Trash")
            .frame(theme::modal_frame())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(format!(
                    "Move {} capture{} to the system trash?",
                    paths.len(),
                    if paths.len() == 1 { "" } else { "s" }
                ));
                ui.label(
                    egui::RichText::new("They can be restored from the trash.")
                        .size(qs_theme::font::SMALL as f32)
                        .color(theme::text_tertiary()),
                );
                ui.add_space(qs_theme::space::MD as f32);
                // `components::button` stretches to the full width, which in a
                // horizontal row pushed the destructive button outside the
                // window and clipped the question text with it.
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        decision = Some(false);
                    }
                    let trash = egui::Button::new(
                        egui::RichText::new("Move to Trash").color(theme::danger_red()),
                    );
                    if ui.add(trash).clicked() {
                        decision = Some(true);
                    }
                });
            });
        // The dialog is keyboard-complete too: ⌘⌫ opened it, so Enter must be
        // able to confirm and Escape to back out without a mouse.
        let (enter, esc) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::Enter),
                i.key_pressed(egui::Key::Escape),
            )
        });
        if !open || esc {
            decision = Some(false);
        }
        if enter {
            decision = Some(true);
        }
        match decision {
            Some(true) => {
                let outcomes = qs_library::trash::move_all(&paths);
                let failed = outcomes.iter().filter(|o| o.error.is_some()).count();
                if failed > 0 {
                    self.write_error = Some(format!(
                        "{failed} of {} captures could not be moved to the trash.",
                        outcomes.len()
                    ));
                }
                for outcome in outcomes.iter().filter(|o| o.error.is_none()) {
                    if let Some(name) = outcome.path.file_name().and_then(|n| n.to_str()) {
                        self.entries.retain(|e| e.file_name != name);
                        if let Some(index) = &mut self.index {
                            index.entries.remove(name);
                        }
                    }
                }
                let _ = self.persist();
                self.selected.clear();
                self.confirm_delete = false;
            }
            Some(false) => self.confirm_delete = false,
            None => {}
        }
    }
}

/// Five-point star drawn as a path.
///
/// The bundled fonts have no U+2605/U+2606, so a text star rendered as nothing at
/// all — the favourite marker was invisible until this.
fn paint_star(
    painter: &egui::Painter,
    centre: Pos2,
    radius: f32,
    color: Color32,
    filled: bool,
) {
    let mut points = Vec::with_capacity(10);
    for i in 0..10 {
        let angle = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
        let r = if i % 2 == 0 { radius } else { radius * 0.45 };
        points.push(Pos2::new(
            centre.x + angle.cos() * r,
            centre.y + angle.sin() * r,
        ));
    }
    let stroke = egui::Stroke::new(1.2, color);
    if filled {
        painter.add(egui::Shape::convex_polygon(points, color, egui::Stroke::NONE));
    } else {
        painter.add(egui::Shape::closed_line(points, stroke));
    }
}

enum RowHit {
    None,
    Click { cmd: bool, shift: bool },
    Star,
    Open,
}
