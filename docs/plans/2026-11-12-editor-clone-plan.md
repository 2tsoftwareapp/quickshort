# Quickshort Editor — Kế hoạch triển khai clone tính năng Snagit

- **Ngày:** 2026-11-12
- **Bản:** r1 (kế hoạch, chưa code)
- **Căn cứ:** `2026-11-12-editor-feature-audit-r1.md` (audit hiện trạng)
- **Repo:** `/data/dev/work/rewrite/quickshort`, nhánh hiện tại `feat/snagit-p0`

---

## 1. Mục tiêu

Đưa editor Quickshort đạt ngang Snagit về **chỉnh ảnh + chú thích + quản lý đối tượng**, giữ nguyên kiến trúc hiện có (vector document, non-destructive, `.qsdoc`, `qs-theme`).

### Trong phạm vi
- Nhóm Image: crop, resize canvas, canvas color, trim, rotate, flip
- Selection tool và các thao tác theo vùng chọn
- Effects: border, shadow, torn/edge, watermark, color adjust, perspective/reflection (một phần)
- Tools còn thiếu: Magnify, Spotlight, Stamp, Eraser, Move
- Quản lý đối tượng: layers panel, lock/hide, opacity, align/distribute, group, multi-select
- Quick Styles (preset + style tự lưu)
- Text nâng cao: italic, underline, font family, outline
- An toàn dữ liệu: cảnh báo đóng, Save As, autosave/recovery

### Ngoài phạm vi (làm sau, không nằm trong kế hoạch này)
- Video/GIF editor (Snagit có; Quickshort đã có `qs-app` record riêng)
- Library/tray đầy đủ với tags/keywords/flags/search toàn cục (đề xuất phase riêng sau)
- Share target/upload, print
- Page Curl, Reflection (macOS-only ở Snagit), Color Replacement

### Ràng buộc
- Không commit/push khi chưa được yêu cầu.
- Mọi màu UI đi qua `qs-theme` (guard `no_literal_colors` đang bật).
- `.qsdoc` cũ phải mở được: mọi field mới có `#[serde(default)]` + bump `DOC_VERSION` + nhánh migrate.
- Không bake hiệu ứng vào pixel của `base.png` — luôn tái tạo được từ doc.

---

## 2. Nguyên tắc kiến trúc (áp dụng cho mọi phase)

1. **Một chiều phụ thuộc:** `qs-doc` (model) ← `qs-edit` (logic) ← `qs-render` (vẽ) ← `qs-editor` (UI). UI không tự sửa `Document`, luôn qua method `Editor::*` để có undo.
2. **Undo bằng snapshot:** mọi mutation phải nằm trong `Snap` (đã có `objects/view/background/step_start`). Thêm state mới ⇒ thêm vào `Snap`.
3. **Non-destructive:** hiệu ứng là object hoặc field của `Document`, render lại từ `base`.
4. **Doc version:** `DOC_VERSION` 1 → 2 ở phase đầu tiên có field mới; `migrate()` xử lý v1.
5. **Test trước, có ảnh hưởng pixel:** mỗi hiệu ứng/render mới cần test trong `qs-render` kiểm pixel cụ thể (pattern đã có: `rect_stroke_and_fill_are_independent`).
6. **UI kit:** control mới dùng `qs-editor::theme` + `qs-app`-style token; không hex.

---

## 3. Thứ tự phase & phụ thuộc

```
P0 An toàn dữ liệu        (độc lập, làm ngay)
 └─ P1 Image ops          (crop / canvas / trim / rotate / flip)
     └─ P2 Selection      (vùng chọn → mở khoá P4 region ops)
         ├─ P3 Effects    (border/shadow/watermark/color)
         └─ P4 Spot/Magnify/Stamp/Eraser
P5 Object mgmt (layers, multi-select, align, group, opacity, lock)
P6 Quick Styles
P7 Text nâng cao
P8 Library/Share/Print (sau, ngoài scope chính)
```

Ước lượng: **P0 1d · P1 3d · P2 1d · P3 5d · P4 4d · P5 5d · P6 1.5d · P7 1.5d** ≈ **22 ngày công** (chưa gồm P8).

---

## 4. Chi tiết từng phase

### P0 — An toàn dữ liệu (1 ngày)

**Vì sao trước:** rẻ nhất, chặn mất dữ liệu. Hiện `dirty` có nhưng đóng cửa sổ là mất.

| Việc | File | Cách làm |
|---|---|---|
| Cảnh báo đóng khi `dirty` | `qs-editor/src/app.rs` | `handle_close` + `ViewportCommand::Close`; dialog 3 nút **Save / Discard / Cancel** dựng bằng egui window (không dùng dialog hệ điều hành để test được) |
| Save As + chọn đường dẫn | `app.rs`, `Cargo.toml` | thêm `rfd = "0.15"` (portal trên Wayland, native trên macOS/Windows); `save_as()` |
| Mở file (Open) | `app.rs` | nút Open + `Ctrl+O`, dùng `rfd::FileDialog` |
| Autosave + recovery | `app.rs`, `qs-doc/src/qsdoc.rs` | ghi `.qsdoc` vào `$XDG_STATE_HOME/quickshort/editor/autosave-<pid>.qsdoc` mỗi 20s khi `dirty`; xoá khi save thành công; khi khởi động phát hiện file mồ côi → hỏi khôi phục |
| Tiêu đề cửa sổ có `•` khi dirty | `app.rs` | `ViewportCommand::Title` |
| Chặn `Ctrl+S` khi chưa có đường dẫn | `app.rs` | `save()` gọi `save_as()` nếu `doc_path == None` (bỏ hành vi ghi `screenshot.qsdoc` vào CWD) |

**Test:** unit cho logic quyết định (dirty → prompt; autosave path; recovery scan). Dialog/close không test tự động được → ghi `NOT VERIFIED` và có script chụp màn hình.
**Acceptance:** sửa ảnh → đóng → hộp thoại hiện; Cancel giữ cửa sổ; Save ghi đúng file; kill -9 rồi mở lại → có đề nghị khôi phục.

---

### P1 — Image ops (3 ngày)

**P1.1 Crop tool (0.5–1d)** — model đã sẵn (`Editor::set_view`, test `crop_is_one_undo_step`)

- `qs-edit`: thêm `Tool::Crop`. `pointer_down/move/up` tạo rect crop đang kéo (state `Drag::Crop { start }` + `crop_preview: Option<RectF>`). `pointer_up` → `set_view(rect)` (đã có undo) và quay về `Tool::Select`.
- `qs-editor`: nút Crop trong capsule tool (nhóm mới "Image"); vẽ overlay tối ngoài rect, 8 handle, tỉ lệ tự do; phím `K`.
- Cho phép **kéo vượt biên base** (extend canvas) — `view` lớn hơn `base_size` đã được render hỗ trợ qua `background`.
- **Test:** crop tạo đúng `view`; undo trả lại; crop extend hiển thị `background`; crop có thể kéo ngược (flip) không vỡ.

**P1.2 Canvas resize + Canvas color (0.5d)**
- UI: nút "Canvas…" mở popover 4 ô số (T/R/B/L) + swatch màu nền + nút "Fit to image".
- `qs-edit`: `set_view` + `set_background` (cả hai đã có); thêm `resize_canvas(t, r, b, l)` gộp thành **một** bước undo.
- `qs-editor`: swatch dùng `qs_theme::ANNOTATION` + color picker RGB/HEX đơn giản.

**P1.3 Trim (0.5d)**
- Thuật toán: quét viền ngoài, tìm dải đồng nhất (dung sai ±6 mỗi kênh, hoặc alpha=0), trim đến khi gặp pixel khác. Bỏ qua nếu trim > 60% diện tích (an toàn).
- Đặt ở `qs-edit::trim_view(&Document) -> Option<RectF>` (thuần, test được không cần UI).
- **Test:** ảnh có viền trắng 20px → trim còn đúng phần nội dung; ảnh toàn màu → không trim; ảnh đã sát viền → `None`.

**P1.4 Rotate / Flip (1d)**
- Model: thêm vào `Document`:
  ```rust
  #[serde(default)]
  pub transform: Orientation, // None | Rot90 | Rot180 | Rot270 | FlipH | FlipV | ...
  ```
- Render: `qs-render::render` áp transform **sau khi** vẽ base, **trước khi** vẽ object? Không — object phải xoay cùng ảnh ⇒ áp transform lên toàn bộ: hoán đổi trục khi map `xf`, đổi `output_size`. Cách sạch nhất: render vào pixmap theo toạ độ doc, rồi xoay pixmap kết quả ở cuối (`Pixmap::rotate90` / `flip_h`). Object đã nằm trong pixmap nên xoay cùng.
- **Test:** ảnh 30×20 có 1 pixel đánh dấu → Rot90 ra 20×30 và toạ độ đúng; Rot90 bốn lần = gốc; flip hai lần = gốc.
- Cảnh báo: `Blur` object tham chiếu pixel xung quanh nên xoay ở cuối vẫn đúng (blur áp trong toạ độ doc trước khi xoay).

**Acceptance P1:** crop/resize/rotate/flip đều undo được 1 bước; `.qsdoc` cũ vẫn mở; `DOC_VERSION = 2`.

---

### P2 — Selection tool + vùng chọn (1 ngày)

- `qs-edit`: thêm `Tool::SelectRegion` (chữ `M`). Trả về `Document.region: Option<RectF>` (serde default). `Snap` thêm `region`.
- Selection không phải object → không tham gia z-order; vẽ overlay marching-ants (dùng `segments` xen kẽ đen/trắng, không màu theme).
- Mở khoá: Trim-to-selection, Effects-on-selection (P3.b), blur vùng chọn, copy vùng chọn ra clipboard.
- **Test:** chọn vùng → `region` set; `Esc` xoá; undo khôi phục; render không đổi pixel khi chỉ có selection.

---

### P3 — Effects (5 ngày)

Thiết kế: `Document.effects: Effects` (serde default) — áp **quanh** ảnh, sau khi render xong xuôi.

```rust
pub struct Effects {
    pub border: Option<Border>,      // width, rgb, inset, radius
    pub shadow: Option<Shadow>,      // blur, offset, opacity, spread
    pub watermark: Option<Watermark>,// text, pos (9 anchor), opacity, scale
    pub edge: Option<Edge>,          // None | Torn | Fade | Shadow
    pub color: Option<ColorAdjust>,  // brightness, contrast, saturation
}
```

**P3.1 Border + Shadow (2d)**
- Border: mở rộng canvas `n` px mỗi phía, vẽ khung bo góc. Cần đổi `output_size` (cộng `2*inset`) và dịch toàn bộ nội dung vào trong.
- Shadow: render ảnh vào pixmap tạm, làm mờ alpha (dùng lại `gaussian` trên kênh alpha), vẽ lệch `offset`, rồi vẽ ảnh chính lên.
- **Test pixel:** border 10px đỏ → viền đúng 10px, kích thước output +20px mỗi trục; shadow: pixel ngoài ảnh có alpha < 255 và tối dần về biên.

**P3.2 Watermark + Edge/torn (2d)**
- Watermark: text (dùng `qs-render::text::layout`) + 9 vị trí neo + opacity + scale theo cạnh ngắn.
- Torn/Fade edge: mask alpha theo đường nhiễu (deterministic seed để test ổn định).
- **Test:** watermark xuất hiện ở 4 góc đúng; torn edge làm pixel biên thành alpha=0.

**P3.3 Color adjust (1d)**
- brightness/contrast/saturation trên toàn pixmap sau khi vẽ object (chỉ ảnh, không ảnh hưởng object? Snagit áp toàn ảnh — ta áp toàn output, ghi rõ trong doc).

**UI:** nút "Effects" ở top bar mở **SidePanel phải** (dùng `egui::Panel::right`) chứa các nhóm gập được; có "Apply to selection" khi có `region`.

**Acceptance P3:** bật/tắt từng effect không làm hỏng object; undo 1 bước mỗi lần đổi; `.qsdoc` lưu/đọc lại đúng; render lại khớp byte-for-byte sau round-trip.

---

### P4 — Magnify / Spotlight / Stamp / Eraser (4 ngày)

| Tool | Thiết kế | Ghi chú |
|---|---|---|
| **Magnify** | `ObjectKind::Magnify { r: RectF, zoom: f32, shape: Rect/Ellipse, stroke }`. Render: copy vùng `r` từ pixmap hiện tại (dùng `pm.subpixmap`) và vẽ scaled vào cùng chỗ (hoặc vị trí `dst`). | Phải copy **trước** khi vẽ chính nó để tránh đệ quy |
| **Spotlight** | `ObjectKind::Spotlight { r, dim: f32, blur: f32 }`. Render: làm tối toàn pixmap 1 lớp alpha, rồi "đục" vùng `r` về sáng (hoặc vẽ lại vùng sáng từ bản copy). | Cần bản copy trước khi dim |
| **Stamp** | `ObjectKind::Stamp { r, asset: String, tint: Option<u32> }` + thư mục `assets/stamps/*.png` nhúng bằng `include_bytes!`. Bộ đầu: arrow-right, arrow-left, cursor, check, cross, star, number-badge, "Click", "Enter". | Tint: nhân màu giữ alpha |
| **Eraser** | `Tool::Eraser` xoá **pixel của object đang chọn**? Snagit xoá pixel ảnh. Ta làm: `ObjectKind::Eraser { pts, width }` trong doc → render đục lỗ (alpha=0) trên pixmap; đơn giản, non-destructive. | Đục lỗ làm lộ `background` |

- **Test:** magnify phóng đúng hệ số (kiểm pixel trung tâm); spotlight vùng ngoài tối đúng alpha, vùng trong không đổi; stamp vẽ đúng kích thước; eraser tạo alpha=0 và lộ nền.
- **UI:** 4 nút thêm vào capsule (nhóm thứ 4) + inspector riêng cho từng loại.

---

### P5 — Quản lý đối tượng (5 ngày)

**P5.1 Multi-select (1.5d)** — refactor rủi ro, tách riêng
- `Editor.selected: Option<ObjectId>` → `selection: Vec<ObjectId>` (giữ API cũ `selected()` trả `first`).
- Sửa toàn bộ call-site: `update_selected`, `set_color`, handles, inspector.
- Shift+click thêm/bớt; marquee select trên canvas trống bằng `Tool::Select`.
- **Test:** chọn 2 object, đổi màu → cả hai đổi, 1 bước undo; xoá nhiều object 1 bước.

**P5.2 Layers panel (1.5d)**
- `egui::Panel::right` (dùng chung với Effects panel, chia tab hoặc gập).
- Mỗi dòng: icon loại + label + mắt (visible) + khoá (locked) + kéo đổi z-order.
- Nối `set_visible`/`set_locked`/`reorder` đã có trong `qs-edit` (hiện chưa UI nào gọi).
- Double-click dòng → select + zoom tới object.

**P5.3 Align / Distribute (1d)**
- 6 align (left/hcenter/right/top/vcenter/bottom) + 2 distribute (h/v).
- Chỉ bật khi chọn ≥2 (align) / ≥3 (distribute). Align theo bounding box chung.
- Thuần hình học → đặt ở `qs-edit::align.rs`, test không cần UI.

**P5.4 Group (0.5d)**
- `Object.group: Option<u32>` (serde default) + `Document.next_group`.
- `Ctrl+G` / `Ctrl+Shift+G`. Click vào thành viên → chọn cả nhóm. Group là "tập hợp logic", không phải object cha ⇒ giữ nguyên z-order hiện tại (đơn giản, tránh phá render).
- **Test:** group 3 object → di chuyển 1 → cả 3 dịch; ungroup → độc lập.

**P5.5 Opacity object (0.5d)**
- `Object.opacity: f32` (serde default 1.0). Render: vẽ object vào pixmap tạm rồi blend theo opacity (chỉ áp cho nhánh cần; nếu object đã có alpha thì nhân).

---

### P6 — Quick Styles (1.5 ngày)

- `Style` đã là struct thuần ⇒ thêm `Serialize/Deserialize`.
- Preset nhúng: 8 preset (red arrow, yellow highlight, dark callout, blue step…) trong `styles/presets.json` (`include_str!`).
- Style tự lưu: `~/.config/quickshort/editor-styles.json`.
- UI: hàng swatch "style chip" ngay dưới tool capsule khi có tool active; chuột phải object → "Save style".
- **Test:** preset → `Style` khớp; lưu/đọc file; áp style cho object đang chọn đổi đúng thuộc tính.

---

### P7 — Text nâng cao (1.5 ngày)

- `TextStyle` thêm: `italic: bool`, `underline: bool`, `family: Family` (`Sans|Serif|Mono` — dùng font hệ thống qua egui, nhúng fallback).
- Italic: `qs-render::text` áp shear transform khi vẽ; trong egui dùng `FontId` + skew qua `TextShape` hoặc chọn font italic nếu có.
- Outline: `TextStyle.outline: Option<Stroke>` → vẽ text nhiều hướng lệch 1px màu viền trước, rồi vẽ chữ.
- Underline: đường 1.5px dưới baseline từng dòng.
- **Test:** italic khác regular (so khớp pixel); underline nằm dưới đáy layout; outline bao quanh glyph.

---

### P8 — Library / Share / Print (ngoài scope, ghi nhận)

Chỉ ghi nhận để không quên: cần `qs-app` side (thư viện, tags/keywords, search dùng `Meta.ocr_text`), share target, print. Đề xuất làm thành dự án riêng sau khi P0–P7 xong.

---

## 5. Definition of Done (mỗi phase)

1. `cargo test -p qs-doc -p qs-edit -p qs-render -p qs-editor -p qs-theme` xanh.
2. Guard `no_literal_colors` xanh (không hex mới ngoài `qs-theme`).
3. `.qsdoc` v1 cũ mở được, không mất object.
4. Undo/redo: mỗi thao tác mới = đúng **một** bước.
5. Ảnh chụp trước/sau trong `~/.agents/outputs/quickshort/reports/`, đo pixel khi có thể (thay cho vision đang hết quota).
6. Không `cargo fmt` tràn file ngoài scope (đã từng xảy ra — chỉ fmt file mình sửa).

---

## 6. Rủi ro & cách chặn

| Rủi ro | Mức | Chặn |
|---|---|---|
| Multi-select refactor phá inspector/handles | Cao | Làm P5.1 thành phase riêng, có test hồi quy cho mọi tool trước khi merge |
| Effects làm `output_size` lệch, hỏng mapping toạ độ | Cao | Test pixel cho từng effect + test round-trip render→save→load→render |
| Transform (rotate) tương tác xấu với `Blur`/`Magnify` | Vừa | Xoay ở bước cuối, sau khi mọi object đã vẽ; test combo |
| Bump `DOC_VERSION` làm hỏng file cũ | Vừa | Test mở file v1 mẫu trong CI; `migrate` idempotent |
| `rfd` trên Wayland/Hyprland cần portal | Vừa | Fallback `zenity`/`kdialog`; test tay trên máy |
| Autosave ghi đĩa liên tục | Thấp | Chỉ khi dirty + debounce 20s + file theo pid |

---

## 7. Cần bạn quyết trước khi bắt đầu

1. **Phạm vi:** làm đủ P0–P7 (~22 ngày), hay cắt P8 và một phần P4/P5?
2. **Ưu tiên:** bắt đầu P0 → P1 (an toàn + image ops) như đề xuất, hay nhảy thẳng vào P3 Effects vì khác biệt thị giác lớn nhất?
3. **Selection semantics:** vùng chọn là *state của document* (lưu vào `.qsdoc`) hay *state của editor* (không lưu)? Đề xuất: **không lưu** — giống Snagit, tránh rác trong file.
4. **Eraser:** chấp nhận "đục lỗ lộ nền" (non-destructive) hay cần xoá đúng pixel ảnh?
