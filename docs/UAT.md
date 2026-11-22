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
