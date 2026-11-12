# Quickshort Editor — Kế hoạch chi tiết phần còn lại (P1.2 → P7/P8)

- **Ngày lập:** 2026-11-12
- **Tác giả:** Hermes Agent
- **Dự án:** Quickshort Editor (Snagit feature parity)
- **Repo:** `/data/dev/work/rewrite/quickshort` (nhánh `feat/snagit-p0`)
- **Trạng thái hiện tại:**
  - **P0 (An toàn dữ liệu):** ĐÃ HOÀN THÀNH (Close warning dialog, Save As, Open, 20s debounce autosave + crash recovery, dirty bullet indicator, Ctrl+S redirect).
  - **P1.1 (Crop tool):** ĐÃ HOÀN THÀNH (Tool::Crop, dragging preview, commit to set_view, undo/redo 1 step, UI overlay, hotkey K).
  - **P2 (Region Selection):** ĐÃ HOÀN THÀNH (Tool::SelectRegion, Document.region, DOC_VERSION bump v1->v2, Snap.region undo/redo, dashed overlay, hotkey M).

---

## 1. Tổng quan lộ trình các phần còn lại

| Phase | Nội dung | Module chính | Thời lượng ước lượng | Trọng tâm kiểm thử |
|---|---|---|---|---|
| **P1.2** | Canvas Resize + Canvas Color | `qs-edit`, `qs-editor` | 0.5 ngày | Test view expand/shrink 1 undo step, đổi màu nền |
| **P1.3** | Auto Trim Canvas | `qs-edit`, `qs-editor` | 0.5 ngày | Quét viền đơn sắc/alpha, dung sai ±6, bảo vệ max 60% |
| **P1.4** | Rotate 90°/180°/270° & Flip H/V | `qs-doc`, `qs-render`, `qs-editor` | 1.0 ngày | Toạ độ xoay không lệch object, round-trip 360° |
| **P3.1** | Effects: Border & Drop Shadow | `qs-doc`, `qs-render`, `qs-editor` | 2.0 ngày | Test pixel viền bo góc, gaussian shadow alpha |
| **P3.2** | Effects: Watermark & Torn Edge | `qs-doc`, `qs-render`, `qs-editor` | 2.0 ngày | 9-anchor positioning, noise-mask viền rách |
| **P3.3** | Effects: Color Adjustments | `qs-render`, `qs-editor` | 1.0 ngày | Brightness, Contrast, Saturation post-processing |
| **P4.1** | Tool: Magnify Lens | `qs-doc`, `qs-render`, `qs-editor` | 1.0 ngày | Zoom 1.5x–4x, hình tròn/chữ nhật, không đệ quy |
| **P4.2** | Tool: Spotlight | `qs-doc`, `qs-render`, `qs-editor` | 1.0 ngày | Dim exterior alpha, đục sáng vùng focus |
| **P4.3** | Tool: Stamp Library | `qs-doc`, `qs-render`, `qs-editor` | 1.0 ngày | Bộ vector/png stamp nhúng, tinting màu |
| **P4.4** | Tool: Eraser | `qs-doc`, `qs-render`, `qs-editor` | 1.0 ngày | Non-destructive punch-through lộ màu background |
| **P5.1** | Multi-select & Transform | `qs-edit`, `qs-editor` | 1.5 ngày | Selection Vec<ObjectId>, move/delete/color hàng loạt |
| **P5.2** | Layers Panel | `qs-editor` | 1.5 ngày | Panel danh sách object, reorder, visibility, lock |
| **P5.3** | Align & Distribute | `qs-edit`, `qs-editor` | 1.0 ngày | 6 kiểu căn lề, 2 kiểu giãn đều, bounding box |
| **P5.4** | Group / Ungroup | `qs-doc`, `qs-edit`, `qs-editor` | 0.5 ngày | Nhóm logic, Ctrl+G, Ctrl+Shift+G |
| **P5.5** | Per-object Opacity | `qs-doc`, `qs-render`, `qs-editor` | 0.5 ngày | Slider 0–100%, blend pixmap |
| **P6** | Quick Styles (Presets) | `qs-doc`, `qs-editor` | 1.5 ngày | Lưu preset mặc định + custom config json |
| **P7** | Typography nâng cao | `qs-render`, `qs-editor` | 1.5 ngày | Italic shear, underline, font family, outline stroke |
| **P8** | Clipboard & Region Export | `qs-edit`, `qs-editor` | 1.0 ngày | Copy vùng chọn, Flatten annotations |

---

## 2. Chi tiết từng Phase còn lại

### Phase 1: Hoàn tất Image Operations (P1.2, P1.3, P1.4)

#### P1.2 — Canvas Resize & Background Color (0.5 ngày)
- **Mục tiêu:** Mở rộng hoặc thu hẹp canvas thủ công theo 4 phía (Top, Right, Bottom, Left) và thay đổi màu nền canvas.
- **Model & Logic (`qs-edit`):**
  - Thêm method `Editor::resize_canvas(&mut self, top: f32, right: f32, bottom: f32, left: f32)`:
    - Điều chỉnh `self.doc.view`: `x -= left`, `y -= top`, `w += left + right`, `h += top + bottom`.
    - Ghi nhận **đúng 1 step Undo** (thay vì 2 step nếu gọi riêng lẻ).
  - Sử dụng `Editor::set_background(&mut self, color: ColorF)` đã có.
- **UI (`qs-editor`):**
  - Nút "Canvas…" trên thanh toolbar hoặc context bar mở Dialog/Popover:
    - 4 ô nhập số (px): Top, Right, Bottom, Left (hỗ trợ nhập âm để xén bớt).
    - Lựa chọn Anchor: 9 ô vuông (Top-Left, Center, v.v.) để tự động phân bổ padding.
    - Swatch màu nền: Trong suốt, Trắng, Đen, Màu tuỳ chọn (`qs_theme`).
    - Nút "Fit Canvas to Objects": Tự động mở rộng view bao trọn các annotation ngoài biên.
- **Acceptance Criteria & Test:**
  - Unit test `resize_canvas_expands_and_undoes_in_one_step`: View thay đổi toạ độ chính xác, Undo trả về view ban đầu.
  - Test màu nền hiển thị đúng trong `qs-render` khi view lớn hơn `base.png`.

#### P1.3 — Auto Trim Canvas (0.5 ngày)
- **Mục tiêu:** Tự động cắt bỏ viền trắng/đen hoặc viền trong suốt xung quanh ảnh.
- **Model & Logic (`qs-edit`):**
  - Hàm thuần `qs_edit::trim::calculate_trim_rect(doc: &Document, tolerance: u8) -> Option<RectF>`.
  - Quét 4 cạnh của base pixmap:
    - Lấy màu mẫu tại 4 góc.
    - Duyệt từ ngoài vào trong chừng nào pixel còn nằm trong khoảng sai số màu (mặc định delta RGB <= 6 hoặc alpha == 0).
    - Dừng lại khi gặp chi tiết ảnh khác màu.
  - Cơ chế an toàn (Safety Guard): Nếu vùng trim vượt quá 60% diện tích ảnh gốc, huỷ bỏ thao tác (trả về `None`) để tránh xén nhầm ảnh đồng màu.
  - Khi người dùng bấm Trim: gọi `Editor::set_view(trimmed_rect)`.
- **UI (`qs-editor`):**
  - Nút "Trim" hoặc menu item "Trim Canvas Edges".
- **Acceptance Criteria & Test:**
  - Test ảnh 100x100 có viền trắng 10px -> trim chính xác thành 80x80.
  - Test ảnh đồng màu 100% -> trả về `None`, không làm mất ảnh.

#### P1.4 — Rotate & Flip (1.0 ngày)
- **Mục tiêu:** Xoay ảnh 90°, 180°, 270° hoặc lật gương ngang/dọc không phá huỷ vector annotations.
- **Model (`qs-doc`):**
  - Enum `Orientation`: `Normal`, `Rotate90`, `Rotate180`, `Rotate270`, `FlipHorizontal`, `FlipVertical`.
  - Field `Document.orientation: Orientation` với `#[serde(default)]`.
- **Render (`qs-render`):**
  - Áp dụng phép biến đổi lên pixmap kết quả sau khi render toạ độ doc (hoặc chuyển đổi toạ độ khi export):
    - Đổi chiều `output_size` khi xoay 90°/270°.
    - Đảm bảo các annotation dạng text, blur, arrow xoay đồng bộ với base.
- **UI & Undo:**
  - Các nút Rotate CW (`Ctrl+R`), Rotate CCW (`Ctrl+Shift+R`), Flip H, Flip V.
  - Thao tác xoay/lật là 1 bước undo trong `Snap`.
- **Acceptance Criteria & Test:**
  - Test xoay 4 lần 90° trả về đúng pixel gốc.
  - Test flip ngang 2 lần bảo toàn toạ độ.

---

### Phase 3: Effects Pipeline (P3.1, P3.2, P3.3)

#### P3.1 — Border & Drop Shadow (2.0 ngày)
- **Model (`qs-doc`):**
  ```rust
  #[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
  pub struct Effects {
      pub border: Option<BorderEffect>,
      pub shadow: Option<ShadowEffect>,
      pub watermark: Option<WatermarkEffect>,
      pub edge: Option<EdgeEffect>,
      pub color_adjust: Option<ColorAdjustEffect>,
  }
  ```
- **Border Effect:**
  - Thuộc tính: `width: f32`, `color: ColorF`, `radius: f32`, `inset: bool`.
  - Mở rộng canvas thêm `width * 2` mỗi chiều (nếu không phải inset).
- **Shadow Effect:**
  - Thuộc tính: `blur_radius: f32`, `offset: Vec2F`, `opacity: f32`, `color: ColorF`.
  - Render shadow qua bộ lọc gaussian trên kênh alpha, dịch chuyển theo offset, vẽ dưới nền nội dung chính.
- **UI:** Right SidePanel mục "Effects" với toggle switch bật/tắt từng hiệu ứng.

#### P3.2 — Watermark & Torn Edge (2.0 ngày)
- **Watermark:**
  - Text bản quyền (ví dụ: "CONFIDENTIAL", tên công ty, ngày giờ).
  - Căn vị trí 9 neo (TopLeft, Center, BottomRight, v.v.), slider Opacity và Font Scale.
- **Torn Edge (Viền xé rách):**
  - Tạo mask alpha với đường sóng/nhiễu (deterministic perlin noise hoặc pseudo-random seed cố định).
  - Tùy chọn 4 cạnh xé: Top, Bottom, Left, Right.

#### P3.3 — Color Adjustments (1.0 ngày)
- **Bộ lọc ảnh:** Brightness (-100% đến +100%), Contrast (-100% đến +100%), Saturation (0% đến 200%).
- Áp dụng trên toàn bộ pixmap kết quả bằng shader hoặc pixel loop SIMD trong `qs-render`.

---

### Phase 4: Công cụ chú thích đặc thù (P4.1 → P4.4)

#### P4.1 — Magnify Lens (Kính lúp) (1.0 ngày)
- `ObjectKind::Magnify { rect: RectF, zoom: f32, shape: ShapeKind, stroke: Stroke }`.
- Lấy sub-pixmap tại vùng `rect`, phóng to theo tỉ lệ `zoom`, vẽ lại vào thấu kính với viền bo tròn hoặc chữ nhật.

#### P4.2 — Spotlight (Làm nổi bật) (1.0 ngày)
- `ObjectKind::Spotlight { rect: RectF, dim_opacity: f32, blur: f32 }`.
- Phủ một lớp màu đen bán trong suốt (`dim_opacity`) lên toàn bộ canvas, đục rỗng (punch-out) vùng `rect` để giữ nguyên độ sáng.

#### P4.3 — Stamp (Con dấu / Icon thư viện) (1.0 ngày)
- Thư mục nhúng vector/PNG: Cursors (chuột), Badges (Approved, Draft, Verified), Numbers, App Icons.
- Cho phép tint màu tuỳ chọn.

#### P4.4 — Eraser (Tẩy xoá phi huỷ) (1.0 ngày)
- `ObjectKind::Eraser { points: Vec<PtF>, width: f32 }`.
- Thay vì ghi đè pixel gốc, Eraser lưu vết cọ và khi render sẽ blend chế độ `Clear`/`DstOut` để lộ màu nền `background`.

---

### Phase 5: Quản lý đối tượng chuyên nghiệp (P5.1 → P5.5)

#### P5.1 — Multi-selection & Marquee Select (1.5 ngày)
- Đổi `selected: Option<ObjectId>` thành `selection: Vec<ObjectId>`.
- Kéo chuột trên canvas trống ở `Tool::Select` để tạo khung marquee chọn nhiều đối tượng.
- Giữ phím `Shift` để click thêm/bớt đối tượng.
- Thao tác di chuyển, đổi màu, xoá áp dụng đồng thời cho tất cả các đối tượng được chọn trong 1 bước undo.

#### P5.2 — Layers Panel (1.5 ngày)
- Panel bên phải hiển thị danh sách các layer đối tượng theo thứ tự z-order.
- Icon phân biệt loại đối tượng (Text, Shape, Arrow, Callout, Blur, v.v.).
- Nút Toggle Ẩn/Hiện (con mắt) và Khoá/Mở khoá (ổ khoá).
- Kéo thả để thay đổi z-order.

#### P5.3 — Alignment & Distribution (1.0 ngày)
- Căn lề: Căn trái, Căn giữa ngang, Căn phải, Căn trên, Căn giữa dọc, Căn dưới.
- Giãn đều: Giãn đều khoảng cách ngang, Giãn đều khoảng cách dọc.
- Bật khi chọn >= 2 đối tượng (căn lề) hoặc >= 3 đối tượng (giãn đều).

#### P5.4 — Grouping (`Ctrl+G` / `Ctrl+Shift+G`) (0.5 ngày)
- Nhóm nhiều đối tượng thành một Group logic. Click vào bất kỳ đối tượng nào trong nhóm sẽ chọn cả nhóm.

#### P5.5 — Object Opacity (0.5 ngày)
- Slider điều chỉnh độ trong suốt của từng đối tượng (0% đến 100%).

---

### Phase 6: Quick Styles & Presets (1.5 ngày)
- Hệ thống lưu trữ các phong cách thường dùng cho từng công cụ (màu sắc, độ dày viền, font chữ, shadow).
- 8 Preset mặc định có sẵn cho mỗi công cụ.
- Người dùng có thể lưu style của đối tượng đang chọn thành preset riêng (`~/.config/quickshort/styles.json`).
- Thanh quick style chip hiển thị ngay dưới thanh công cụ khi kích hoạt tool tương ứng.

---

### Phase 7: Typography nâng cao (1.5 ngày)
- Hỗ trợ đầy đủ Font Family (System Sans, Serif, Monospace).
- Nút bấm định dạng nhanh: In đậm (Bold), In nghiêng (Italic - shear transform), Gạch chân (Underline), Gạch ngang (Strikethrough).
- Text Outline: Tạo viền chữ giúp đọc rõ trên các nền ảnh phức tạp.
- Căn lề đoạn văn bản: Trái, Giữa, Phải.

---

### Phase 8: Hoàn thiện tính năng xuất & Clipboard (1.0 ngày)
- Phím tắt `Ctrl+C`:
  - Nếu có vùng chọn (`region`): Copy ảnh của riêng vùng chọn ra hệ thống clipboard.
  - Nếu không có vùng chọn: Copy toàn bộ ảnh đã qua render ra clipboard.
- Tính năng "Flatten All": Gom tất cả các vector annotations thành ảnh nền duy nhất khi cần khoá bản vẽ.

---

## 3. Tiêu chí nghiệm thu (Definition of Done) cho mỗi Phase

1. **Compilation & Quality Gate:**
   - `cargo check --workspace` không có lỗi.
   - `cargo test -p qs-doc -p qs-edit -p qs-render -p qs-editor` vượt qua 100%.
2. **Kiến trúc dữ liệu:**
   - Mọi thay đổi đối với tài liệu phải đảm bảo backward-compatibility: Các file `.qsdoc` phiên bản trước vẫn mở được bình thường.
   - Thao tác chỉnh sửa không làm hỏng tính chất non-destructive của ảnh nền.
3. **Undo/Redo Guarantee:**
   - Mỗi thao tác của người dùng (kể cả thao tác hàng loạt trên nhiều đối tượng) gom gọn trong đúng 1 bước undo/redo.
4. **Theme Guard:**
   - Tuân thủ quy tắc không dùng màu cứng (hardcoded hex color), tất cả UI control sử dụng token từ `qs_theme`.
5. **Git & CI:**
   - Tắt kích hoạt tự động của GitHub Actions CI để tối ưu tài nguyên và quản lý thủ công khi cần.
   - Commit rõ ràng theo từng phase hoàn thành kèm tag `[skip ci]`.
