# Hướng dẫn build & UAT (bản local)

Tài liệu này để test tay các tính năng vừa làm: Library, Paint bucket, Smart Move, Colorize,
Quick Style, và nhóm OCR (đề xuất khung, copy chữ theo vùng, tìm chữ).

## 1. Build

```bash
cd ~/tx/GitHub/quickshort
export PATH="$HOME/.cargo/bin:$PATH"
cargo build --release -p qs-app -p qs-editor
```

Hai binary:

| Binary | Dùng để |
|---|---|
| `target/release/quickshort` | app chạy nền (tray + hotkey chụp màn hình) |
| `target/release/qs-editor` | cửa sổ editor độc lập, mở ảnh hoặc `.qsdoc` |

Chạy editor:

```bash
./target/release/qs-editor                      # canvas trắng 800×600
./target/release/qs-editor ~/Pictures/quickshort/quickshort-<ts>.png
./target/release/qs-editor duong/dan/file.qsdoc
```

Chạy app nền và chụp:

```bash
./target/release/quickshort                 # tray app, đăng ký hotkey
./target/release/quickshort --capture       # chụp vùng chọn ngay
./target/release/quickshort --capture-full  # chụp toàn màn hình, lưu luôn
./target/release/quickshort --delay 3 --capture
./target/release/quickshort --help
```

**Quyền macOS** (bắt buộc, nếu thiếu app sẽ chụp ra ảnh trắng):

- System Settings → Privacy & Security → **Screen Recording** → bật cho `qs-app`
- System Settings → Privacy & Security → **Accessibility** → bật nếu hotkey toàn cục không ăn

## 2. UAT tự động (không cần bấm tay)

```bash
cargo test --workspace                        # toàn bộ: 298 test
cargo test -p qs-editor --test uat_e2e        # 4 test UI ở 4 viewport × 2 theme
cargo test -p qs-editor --test ocr_hints      # OCR thật trên ảnh trong repo (~3s)
cargo test -p qs-render                       # pixel test cho fill / smart move / colorize
```

`uat_e2e` phủ: toolbar + inspector, Library quét thư mục thật, cửa sổ Find (3 trạng thái) và
highlight, luồng Simplify/Cutout/effects/save/combine.

## 3. Kiểm tra tay — từng tính năng

### 3.1 Library (spec 002)

1. Lấy vài ảnh chụp vào `~/Pictures/quickshort` (hoặc mở Library khi đã có ảnh).
2. Trong editor bấm **⌘L** (hoặc pill **Library** ở thanh trạng thái dưới cùng).
3. Thử: gõ vào ô Search (tên file hoặc tag) → danh sách lọc ngay.
4. Filter **Images / Docs**, **Today / Last 7 days / Last 30 days**, **Newest / Oldest / Name**.
5. Chọn một ảnh → **Open** (hoặc double-click) → mở trong editor, không mất undo của tài liệu cũ.
6. Chọn một ảnh → gõ tag vào ô **add tag** → Enter. Thoát app, mở lại → tag còn.
7. Chuột phải... không có; favourite bằng cách bấm **☆** ở đầu mỗi dòng, rồi bật filter **★ Favourites**.
8. Chọn 1–2 ảnh → **Move N to Trash** → xác nhận → kiểm tra Trash của macOS có file.
9. **Reveal** → Finder mở đúng file.

### 3.2 Paint bucket (spec 003)

1. Mở một ảnh screenshot.
2. Bấm **U** (hoặc chọn công cụ Paint Bucket trong nhóm phải của thanh công cụ).
3. Panel phải hiện card **Paint bucket** → kéo **Tolerance** (0 = chỉ khớp tuyệt đối).
4. Click vào một vùng trống (nền trắng, ô nhập liệu) → vùng đó tô màu hiện tại.
5. **⌘Z** một lần → ảnh trở lại y như trước (fill là object, không nung vào ảnh).
6. Mở **Layers** (panel trái) → ẩn object Fill → pixel gốc hiện lại.

### 3.3 Smart Move (spec 003 + 004)

Cách tay:

1. Bấm **Y** → kéo một khung quanh phần tử muốn dời → thả.
2. Click vào vị trí đích → nội dung dời tới đó, vùng cũ được lấp bằng màu nền xung quanh.
3. **⌘Z** một lần → trở lại ban đầu.

Cách có OCR (trên ảnh có chữ):

1. Bấm **Y**, rê chuột lên một dòng chữ → khung đề xuất hiện đúng theo dòng đó.
2. Click để chốt dòng làm nguồn, rồi click vị trí đích.
3. Nếu ảnh chưa nhận dạng xong, đợi ~1–2 giây (Vision chạy nền), khung sẽ hiện.

### 3.4 Colorize (spec 003)

1. Panel phải → **Effects** → bật **Colorize**.
2. **Replace**: màu cần thay; **With**: màu thay thế; **Tolerance**: 0–128; **Strength**: 0–1.
3. Mẹo test: chụp một nút màu xanh, đặt Replace = màu nút, With = đỏ, Strength = 1.
4. Lưu `.qsdoc`, mở lại → thiết lập còn nguyên, ảnh render giống hệt.

### 3.5 Quick Style: yêu thích + lọc (spec 003)

1. Thanh **Quick Styles** dưới toolbar.
2. Chuột phải một preset **tự lưu** (không phải preset mặc định) → **Add to favourites**.
3. Preset có dấu ★ và được xếp lên đầu; khởi động lại app → vẫn còn.
4. Gõ vào ô **Filter** → chỉ còn preset khớp tên (không phân biệt hoa thường).
5. Preset mặc định không favourite được (chúng được tạo lại mỗi lần chạy) — menu nói rõ điều này.

### 3.6 Copy chữ trong vùng chọn (spec 004)

1. Bấm **M** (Select Region) → kéo một khung quanh vùng có chữ.
2. Thanh trạng thái hiện pill **Copy text** → bấm.
3. Toast báo "Copied N lines"; dán ra để kiểm tra đúng thứ tự đọc.
4. Thử khung không có chữ → toast nói rõ "selection contains no recognized text" (khác với "engine unavailable").

### 3.7 Find in capture (spec 004)

1. **⌘F** → hộp thoại nhỏ hiện góc trên.
2. Gõ một từ có trong ảnh → mọi dòng khớp được tô, dòng hiện tại có viền và canvas tự pan tới.
3. **Next / Prev** (hoặc Enter / Shift+Enter) → nhảy vòng qua các kết quả.
4. Gõ từ không tồn tại → "No match"; **Esc** đóng, highlight biến mất.

### 3.8 Library bằng bàn phím (mới)

Library dùng được không cần chuột: **↑/↓** chọn ảnh, **Enter** mở, **⌘⌫** mở hộp xoá,
**Enter** xác nhận / **Esc** huỷ, **Esc** đóng Library. Khi ô Search hoặc ô tag đang focus thì
các phím này nhường lại cho việc gõ.

## 4. Checklist theo kích thước & theme

Với mỗi mục 3.1–3.7, thử ở: **720×600**, **1000×700**, **1280×800**, **1600×1000**, cả **Dark** và **Light**
(đổi theme ở pill góc phải thanh trạng thái). Không chấp nhận: chồng chữ, nút bị cắt, chữ không đọc được.

## 5. Dọn dẹp / reset trạng thái

```bash
# Xoá index Library (sẽ tự quét lại; mất tag/favourite)
rm ~/Pictures/quickshort/.qs-library.json

# Xoá preset Quick Style tự lưu
rm ~/.config/quickshort/styles.json          # Linux
rm "$HOME/Library/Application Support/quickshort/styles.json"   # macOS

# Reset toàn bộ config (theme, accent, save_dir, ocr_lang)
rm "$HOME/Library/Application Support/quickshort/config.toml"
```

`target/` hiện ~28 GB. Nếu cần giải phóng: `cargo clean` — nhưng lần build sau sẽ lâu (10–20 phút).

## 6. Giới hạn đã biết (đừng báo là bug)

- **OCR Linux/Windows** mới được compile-verify, chưa chạy thật: máy này không có `tesseract`, không có Windows.
- **Hover đề xuất khung Smart Move** chưa có UAT tự động ở mức con trỏ (harness headless không dựng được cửa sổ app); phần vẽ đã nằm trong UAT viewport.
- **Find** không sửa chữ, không đọc clipboard — chỉ điều hướng trong ảnh đang mở.
- **Copy text theo vùng** lấy dòng có *tâm* nằm trong khung; dòng nằm nửa trong nửa ngoài sẽ bị bỏ.
- Ảnh chụp ra trắng ⇒ thiếu quyền **Screen Recording**, không phải lỗi render.

## 7. Ghi chú thiết kế sau vòng polish

- Mọi hành động dùng chung `components::pill_button` (nền + viền + hover). `status_pill` chỉ dùng cho
  toggle/filter — trước đây dùng nó cho nút hành động nên nút trông như chữ trơn.
- Hàng filter Library chia hai hàng cố định, không dùng `separator` trong hàng wrap (separator bị
  đẩy xuống đầu dòng mới thành dấu `|` lơ lửng).
- Dialog cách mép cửa sổ 24 px; ở 720×600 trước đây nó phủ kín và đè status bar.
- Chỉ một overlay mở tại một thời điểm: ⌘F đóng Library, ⌘L đóng Find.

- Quick-style bar có nền + hairline + padding riêng; trước đây panel không fill nên hiện thành dải đen
  trên nền sáng. Chip dùng chung ngôn ngữ với `pill_button` (nền, viền, hover), cao 26, bo 6.
- Ở chế độ compact, nút Copy rộng 64 thay vì 54 — 54 làm chữ "Copy" bị cắt ở 720px.
- Hàng selected ở dark dùng accent alpha 72 (light giữ 36) để không chìm vào nền card.

- Properties bar: swatch và width capsule dùng hằng số có tên (`SWATCH`, `TRACK_PAD`,
  `components::CONTROL_H`) thay cho 20/22/26/6.8 rải rác; chiều cao control thống nhất 26 px ở chip,
  nút Library, Open và capsule width.
- Ở chế độ compact, khi tool đang chọn nằm trong menu "•••", nút đó hiện icon của tool (nền accent)
  kèm tooltip "Current tool: …" thay vì ba dấu chấm — trước đây toolbar không cho biết đang chọn gì.
- Dialog xoá có hairline trên hàng nút, cùng cấu trúc với footer Library.

- Status bar cao 34 px (26 px control + 4 px padding hai chiều), pill dùng `CONTROL_H` thay vì 24;
  divider nhóm là hairline 1×16 px màu `divider_vertical` với gap `space::XS`, không dùng
  `ui.separator()` (vốn kế thừa stroke của widget và nặng hơn phần còn lại).
- `switch_row` cao `CONTROL_H + space::XS` (30) và label/toggle cách mép `space::XS` thay vì 2 px.

- Panel Layers: hàng object dùng chung nhịp 34 px (CONTROL_H 26 + padding 4), nền chọn accent wash,
  viền hairline khi hover; nút ẩn/khoá là **icon vector** (Eye/EyeOff, Lock/Unlock) — trước đây dùng
  ký tự ◉/○/🔒 mà font bundled không có nên nút vô hình. Header dùng pill_button cho Group/Ungroup/Align.
- Test tự động mới `crates/qs-editor/tests/ui_tokens.rs`: mọi control cao đúng `CONTROL_H`; tương phản
  primary/card ≥ 4.5 và tertiary/card ≥ 3.0 ở cả Light và Dark.

- Dialog Canvas/Export/Flatten: hành động dùng `pill_button` (Flatten màu đỏ), khoảng cách theo token;
  ⌘⇧C mở Canvas, ⌘⇧E mở Export, ⌘⇧F mở Flatten — ba dialog loại trừ nhau (mở cái này đóng cái kia).
- Canvas: body cao cố định + `ScrollArea` nên không bị cắt đáy ở 720; hàng Apply tách riêng (trước đây
  chữ "Fit to objects" chồng lên "Apply"); anchor picker vẽ lưới 3×3 (glyph ↖↑↗ không có trong font).

- Phím tắt dialog: ⌘⇧C Canvas, ⌘⇧E Export/Share, ⌥⌘F Flatten, ⌘⇧A Align & Distribute, ⌘, Settings.
  ⌘⇧F không dùng được (macOS nuốt) nên Flatten chuyển sang ⌥⌘F. **Esc** đóng dialog đang mở trên cùng.
- Settings: footer có hairline + pill xanh "Done" thay nút full-width; dialog dùng `pill_button_primary`.

### Ma trận bằng chứng đã chụp

| Surface | Light 720 | Light 1100 | Dark 720 | Dark 1100 |
|---|---|---|---|---|
| Library | ✓ | ✓ | ✓ | ✓ |
| Find | ✓ | ✓ | ✓ | ✓ |
| Canvas | ✓ | ✓ | ✓ | ✓ |
| Export & Share | ✓ | ✓ | ✓ | ✓ |
| Flatten | ✓ | ✓ | ✓ | ✓ |
| Settings | ✓ | ✓ | ✓ | ✓ |
| Align & Distribute | ✓ | ✓ | ✓ | ✓ |
| Layers panel | ✓ | ✓ | ✓ | ✓ |
| Combine | ✓ | ✓ | ✓ | ✓ |

Ảnh nằm trong `.zwork/uat/` (không commit). Không còn ô trống. Filmstrip có điều hướng bàn phím: ⌘⇧T mở/đóng tray, ←/→ di chuyển con trỏ
(viền accent), Space đánh dấu (viền xanh), Enter mở Combine khi ≥2 ảnh được đánh dấu.

- Slider/ComboBox dùng style chung: rail 84 px, knob tròn, chiều cao `CONTROL_H` 26, rail lấy
  `bg_pill_track`, phần đã kéo tô accent (`slider_trailing_fill`). Nhãn dài được rút ("Corner radius"
  → "Radius", "Outline width" → "Outline") để hàng không tràn panel 256 px.
- Fixture `.zwork/uat/layers-sample.qsdoc` có sẵn Border/Shadow/Color adjust để panel Effects hiện
  slider mà không cần click (máy này không tổng hợp được chuột).

- Filmstrip: ⌘⇧T mở tray, ←/→ chọn, Space đánh dấu nhiều, Enter mở Combine (hoặc mở ảnh nếu chọn 1).
  Khi tray mở, phím mũi tên thuộc tray (nudge tạm nhường). Panel cao 124 px để không cắt thumbnail.
- `components::dialog_actions`: hàng nút phải canh đúng chiều cao `CONTROL_H` — trước đây
  `with_layout(right_to_left)` chiếm hết chiều cao còn lại, làm Settings/Combine/Canvas cao bất thường.

- Library với **10 captures**: danh sách tự cap theo viewport nên dialog không vượt cửa sổ (kiểm ở
  720×600 đáy 575/600 và 1100×782 đáy ~712/782), status bar vẫn lộ, danh sách cuộn.
- Đã thử tự động hoá kiểm tra tràn bằng `Shape::visual_bounding_rect` và `Context::globally_used_rect`:
  cách đầu trả rect rỗng, cách sau tính cả bóng đổ nên báo sai. Cả hai đã bỏ; bằng chứng tràn hiện
  dựa vào ảnh chụp.

- Test tương tác bàn phím (`tests/ui_keys.rs`): ↓/↑ chọn hàng Library, Enter trả `Open` đúng file,
  Esc đóng; gõ vào Find tới được ô nhập mà không cần click (chứng minh bản sửa auto-focus).
- Test trạng thái lỗi (`tests/ui_states.rs`): index hỏng → notice "corrupt" + vẫn duyệt được 3 ảnh;
  index version mới hơn → read-only + notice "newer"; thư mục không đọc được → lỗi nêu tên thư mục.

- Test thêm: ⌘⌫ mở hộp xác nhận xoá, Esc huỷ và file vẫn còn trên đĩa; ảnh hỏng (`broken.png`) vẫn
  hiện thành hàng, thumbnail rơi về placeholder và view không sập.
- Lưu ý khi viết test bàn phím: `InputState::modifiers` chỉ đổi qua `Event::ModifiersChanged`, nên
  một tổ hợp phím phải gửi kèm event đó **trước** `Event::Key` trong cùng frame.

- Test xoá thật: ⌘⌫ → Enter → file rời thư mục, hàng biến mất; test dọn lại entry trong `~/.Trash`
  để không để rác lại trên máy.
- Logic phím của tray tách thành `qs-editor/src/filmstrip_keys.rs` (hàm thuần + 4 unit test): mũi tên
  di chuyển và kẹp hai đầu, lần bấm đầu chọn item đầu tiên, Space đánh dấu, Enter mở Combine khi ≥2.
  App chỉ còn dịch `TrayAction` sang hành động.

- Test app-level (`tests/ui_app.rs`) dùng `EditorApp::for_tests` + `handle_dialog_keys`: ⌘⇧C/E/⌥⌘F/⌘⇧A/⌘,
  mở đúng dialog và **đóng cái trước**, Esc đóng dialog trên cùng, ⌘⇧T bật/tắt tray. Qua đó phát hiện
  Align/Settings trước đây không đóng dialog khác nên chồng lên nhau.

- Test Combine tạo sheet thật: hai ảnh tạm → `create_combined_sheet()` → tài liệu có thêm object, có
  Step badge, canvas rộng hơn một ảnh, và được đánh dấu chưa lưu. Qua đó sửa luôn việc `unsaved`
  không được cập nhật ngay sau khi tạo sheet.
- `close_guard` giờ kiểm được bằng phím: **⌘W** đi qua đúng guard đó (thêm `request_close`). Test
  app-level xác nhận tài liệu sạch → gửi `ViewportCommand::Close`; tài liệu có sửa → mở hộp
  "Unsaved changes". Trên app thật: ⌘W với tài liệu sạch làm app thoát (0 tiến trình), với tài liệu
  đã xoay (⌘⇧R) thì hiện hộp Save/Discard/Cancel và app vẫn sống.

- Smart Move hover (đề xuất khung) đã kiểm trên app thật: mở ảnh screenshot, bật **Y**, rê chuột lên
  dòng "General" → khung accent bao đúng dòng đó. Ảnh: `.zwork/uat/147-hover-proposal.png`.
- Giới hạn môi trường: trên máy này **chuột di chuyển tới app nhưng nút bấm thì không**
  (`CGEvent` move → tooltip/hover hoạt động; down/up → không có tác dụng, ảnh trước/sau giống hệt
  từng byte). Vì vậy hover-test được còn click-test thì không; `close_guard` vẫn chưa kiểm được vì
  cần sự kiện đóng cửa sổ.
- Trong stack này **không có "agent desktop" của Vercel Labs**: `tool_search` cho desktop/computer-use
  trả 0 tool (đã thử 2 truy vấn), và `orca-computer-use` bị cấm dùng.

- Phím tắt mới: **⌘W** đóng cửa sổ qua `close_guard` (cùng quy tắc với nút đỏ), tránh mất edit.
