# Quickshort Design System

Visual language target: modern AI-IDE surfaces (Devin / Codex / Claude /
Linear / Raycast). Flat layered surfaces, 1 px hairlines, restrained
accent, generous breathing room, instant but animated feedback.

This document is the **single source of truth** for how UI is built in
this repo. Any new surface or component must follow it; any change to it
is a design decision, not a drive-by edit.

---

## 1. Principles

1. **Tokens or nothing.** Every colour, space, radius, font size and
   duration comes from `qs_theme`. A raw number in a draw call is a bug.
2. **One anatomy per component.** A dropdown row looks and behaves the
   same in the settings window (SDF), the editor (egui) and the overlay
   (HUD) — only the rasterizer differs.
3. **Restraint.** Hairlines instead of borders, tint instead of chrome,
   accent only where it means something (primary action, active state).
4. **Feedback everywhere.** Every hit target shows hover within one
   frame and completes its fade within `motion::FAST` (120 ms).

## 2. Tokens (already in `qs_theme`)

| Group | Scale | Rule |
|---|---|---|
| `space` | 2 / 4 / 8 / 12 / 16 / 24 / 32 | Everything aligns to the 4 px grid; `HAIR` (2) is only for hairline insets and tight glyph gaps. |
| `radius` | 4 / 8 / 10 / 12 / 18 / pill | Nested radius = parent radius − padding (concentric corners). |
| `font` | 11 / 12 / 13 / 15 / 22 | `BODY` 13 is the default; `CAPTION` 11 for metadata only, never controls. |
| `motion` | 120 / 200 / 280 ms | Hover uses `FAST`; popovers `BASE`; nothing exceeds `SLOW`. |

## 3. Surface model (layers, bottom → top)

| Layer | Token / colour role | Chrome |
|---|---|---|
| L0 window | `bg_top → bg_bottom` (subtle vertical gradient) | — |
| L1 sidebar / secondary pane | `sidebar` | right hairline divider |
| L2 card / grouped list | `surface` | `border` hairline inside, `radius::LG` (12) |
| L3 raised control | `control` / `track` | `border_soft`, `radius::SM–MD` |
| L4 floating card (popover, toast, capture card) | `popover` | `popover_border` hairline + soft shadow (`fringe`), `radius::LG` |
| L5 modal | `popover` | scrim `black 32 %` (light) / `47 %` (dark) behind |
| L6 HUD (overlay) | `hud` | always dark, floats over arbitrary content |

Shadows: floating layers only, 1 px `popover_border` + `0 8 24 rgba(shadow)`
— never on flat cards. In SDF renderers a shadow is a blurred dark rect
behind the card; in egui it is `egui::Shadow`.

## 4. Spacing rules

| Where | Token |
|---|---|
| Inside a control (label ↔ edge) | `space::SM` (8) v / `MD` (12) h |
| Between sibling controls | `space::SM` (8) |
| Between a section's rows | `space::XS` (4) — rows touch, hairline inside card |
| Between sections / cards | `space::LG` (16) |
| Card / page outer padding | `space::LG` (16) |
| Page margin (window edge) | `space::XL` (24) |
| Toolbar / status bar padding | `space::MD` (12) |

Hit targets: **≥ 28 × 28 px** everywhere, **≥ 24** only inside dense
toolbars where the visual is smaller but the hit rect still reaches 24.

## 5. Radius rules

| Element | Radius |
|---|---|
| Button, dropdown, input | `radius::MD` (10) |
| Floating card, modal, menu | `radius::LG` (12) |
| Segmented control track | pill (255) |
| Thumbnail / media | `radius::SM` (8) |
| Badge, tag, keycap | `radius::XS` (4) |
| Full-height sidebar selection | `radius::MD`, inset `space::SM` from sidebar edge |

Concentric rule: child radius = parent radius − padding. A 12-radius
card with 4 px inset rows uses 8-radius rows.

## 6. Borders & dividers

- Borders are **1 px hairlines drawn inside** the shape (`StrokeKind::Inside`
  / inset SDF stroke) — never outside; outside strokes double the box.
- A card uses `border`, an interactive control `border_soft` →
  `border_hot` on hover.
- Dividers inside a card: full-width 1 px `border` at 60 % alpha, inset
  by the row's left padding (not edge-to-edge through the glyph column).
- Focus ring: 2 px `accent` ring outside the control + 1 px gap — keyboard
  focus only, never on pointer hover.

## 7. Interaction states (all components, all surfaces)

| State | Visual | Motion |
|---|---|---|
| Rest | role colours | — |
| Hover | bg → `surface_hover` / `control_hot`; border_soft → border_hot; cursor `PointingHand` | fade in `motion::FAST` |
| Pressed | hover colour × 0.92 luminance (slightly darker) | instant |
| Active / selected | accent fill or accent-tint (alpha 12–18 %) + accent fg/glyph | `motion::FAST` |
| Disabled | 40 % opacity, no hover, `CursorIcon::NotAllowed` where supported | — |
| Focused (kbd) | state visuals + 2 px accent ring | instant |

Transitions: cross-fade colours on hover in/out — in SDF surfaces store a
`hover_t: f32` per hit target and lerp coverage each frame; in egui use
`ctx.animate_value_with_time` or manual `Instant` lerp. No state may
appear without its transition unless the frame rate can't sustain it.

## 8. Component anatomy

### 8.1 Button
- Height 28 (`space::XL + space::XS` = 24 + 4, matches
  `components::button`); dense toolbar 24–26 (`CONTROL_H`). Padding h
  `space::MD`.
- `radius::MD`, label `font::SMALL` medium weight.
- **Primary**: accent fill (gradient `accent_top → accent_bottom` where
  gradients exist), `on_accent` label; hover = `accent_top` flat lift.
- **Tonal**: accent-tinted fill 12–18 % + 1 px accent border 30 %;
  hover raises both alphas ~50 %.
- **Secondary**: `control` fill, `border_soft`; hover `control_hot`.
- **Ghost**: no fill at rest; hover `surface_hover`. For quiet actions.
- **Destructive**: secondary anatomy, `danger` label; hover `danger` fill
  + `on_accent` label. Never rest-state red fill.
- Icon + label buttons: icon `space::MD` left, gap `space::XS`.

### 8.2 Icon button / toolbar segment
- 28 × 28 hit rect, icon 16 px box (Lucide, `qs-icons`), `radius::SM`.
- Active tool: accent-tint pill; hover: `wash` 11 % (see
  `bg_pill_hover`). Never draw the icon itself differently on hover —
  the pill carries the state.

### 8.3 Toggle switch
- Track 36 × 20, `radius::PILL`, `switch_off` → `accent` when on; thumb
  16 disc, `on_accent`, slides with `motion::FAST`. The SDF settings
  switch already eases its knob — keep that animation as the reference.
- Row anatomy: label `font::BODY` left, switch right-aligned; row
  height 30 (`CONTROL_H + XS`), entire row is the hit target and shows
  the row-hover pill.

### 8.4 Checkbox / radio
- 20 × 20 box, `radius::XS` (radio = circle); `border_soft` rest →
  `border_hot` hover; on = accent fill + `on_accent` check glyph 12 px.
- Row = label + control, whole row clickable, same as switch.

### 8.5 Text input / field
- Height 28, `radius::MD`, `track` fill, `border_soft`; focused:
  `border_hot` + 2 px accent ring; text `font::BODY`, caret accent.
- Placeholder `fg_faint`. IME preedit underline accent (see editor).

### 8.6 Slider
- Track h 4 `radius::PILL` `track`; fill part accent; thumb 14 disc
  `on_accent` + 1 px `border_soft`, grows to 16 on drag.
- Row anatomy: label left, slider mid, live value right `font::SMALL`
  `fg_dim`, mono-ish tabular if available.

### 8.7 Dropdown / select
- Trigger = secondary button anatomy + right chevron (`qs-icons::CHEVRON_DOWN`),
  12 px, `fg_dim`.
- Popup: floating card (L4), `radius::LG`, min-width = trigger width,
  4 px below trigger, `popover_border` + shadow, opens with
  `motion::BASE` fade + 4 px slide-up.
- Rows: 28 px h, `font::BODY`, hover pill `radius::SM` inset 4 px;
  selected row = accent-tint 12 % + accent check right side; keyboard
  arrows walk rows, Enter commits, Esc closes. Modal while open —
  click-anywhere-else dismisses.
- Right-aligned shortcut / meta text `font::CAPTION` `fg_faint`.

### 8.8 Menu / context menu
- Same popup + row anatomy as dropdown; separators = inset hairline
  (rule in §6), never full-bleed.
- Destructive item: `danger` label, hover keeps danger-tint pill.

### 8.9 Segmented control
- Track: pill, `track` fill, `border_soft`; segments equal width,
  `font::SMALL`; selected segment = `pill_top → pill_bottom` raised pill
  with hairline + micro-shadow; inactive hover = `wash` 11 %.

### 8.10 Card / panel
- `surface` fill, `radius::LG`, `border` hairline, padding `space::LG`.
- Section header inside card: `font::CAPTION`, `fg_dim`, baseline aligned
  `space::SM` above the first row (egui has no letter-spacing — skip it).

### 8.11 Sidebar / list row
- Sidebar width 200–220, `sidebar` fill, 1 px right `border` divider.
- Nav row: h 36, `radius::MD`, inset 10 px; icon tile 16 px glyph + 10
  gap + `font::BODY` label; selected = accent fill + `on_accent`;
  hover = `surface_hover`. Row stride 40.
- Grouped list rows (settings pages): h 32 default / 44 with subtitle;
  secondary text `font::CAPTION` `fg_dim`.

### 8.12 Modal / dialog
- Scrim: black at alpha 52 (light) / 120 (dark) — ≈ 20 % / 47 %, matches
  `components::modal_backdrop`. Click-scrim = cancel for non-destructive
  dialogs only.
- Card: `popover`, `radius::LG` (12→14 ok), `popover_border` + shadow,
  min-width 360, max-width 480, padding `space::LG`.
- Anatomy: title `font::TITLE` semibold → body `font::BODY` `fg_dim`
  (max 3 lines) → action row right-aligned, gap `space::SM`:
  [Cancel ghost/secondary] [Primary/Destructive].
- Enter = primary, Esc = cancel. Appear: `motion::BASE` fade + scale
  0.96→1.

### 8.13 Alert / confirm
- Same modal anatomy + 20 px status glyph in the status colour.
  Vendored today: `CIRCLE_CHECK` (success), `INFO`, `X`; vendor
  `triangle-alert` (warning) and `circle-x` (error) from Lucide when
  implementing — do not hand-draw them.
- Destructive confirms name the object ("Delete 3 captures?") and use a
  destructive primary, never a bare "OK".

### 8.14 Toast
- Floating card (L4), W 300 auto-H, `radius::LG`, border + shadow.
  Dock position follows `POPUP_POS` (default `top-right`, macOS-style),
  `widgets::dock_pos` margin = 20 px physical. Multiple toasts stack
  inward 8 px apart.
- Anatomy: 20 px status mark (`mark()` for spinner / `circle-check` /
  `x-circle`) + title `font::BODY` semibold + optional line 2
  `font::CAPTION` `fg_dim`.
- In: `motion::BASE` fade + 8 px rise (native window alpha where
  available). Out: fade `motion::BASE` at TTL 1.8 s success / 4 s error;
  errors persist until dismissed if they carry an action.
- One spinner-to-check transition for long ops (already the pattern in
  `toast.rs`) — keep it.

### 8.15 Tooltip
- `popover` fill, `popover_border`, `radius::SM`, padding 6/8,
  `font::CAPTION`, delay 300 ms, `motion::FAST` fade. Plain text only —
  rich tooltips are popovers, not tooltips.

### 8.16 Scrollbar (custom surfaces)
- Overlay-only: 6 px track, thumb `wash` 35 % → 55 % on hover, `radius::PILL`,
  auto-hide after 800 ms idle, `motion::BASE` fade. Never shrink content
  for a scrollbar.

### 8.17 Empty state
- Centre: 48 px `fg_faint` glyph (Lucide) → title `font::TITLE` →
  body `font::BODY` `fg_dim` → one primary action. Existing
  `components::empty_state` is the reference.

### 8.18 Badge / pill / chip
- h 20, `radius::PILL`, padding 8 h, `font::CAPTION`; tonal fill =
  role colour 12 % + role text. Status dots 8 px + `space::XS` gap.

### 8.19 Keycap (hotkey display/input)
- `keycap` top fill + `keycap_edge` 1 px bottom edge, `radius::XS`,
  `font::CAPTION` label, min-width 24, padding 7 h, 4 px gap between
  keys in a chord (`keycaps_w` is the existing measure). Recording state:
  `border_hot` + accent label.

## 9. Per-surface notes

| Surface | Notes |
|---|---|
| Editor (egui) | `components.rs` is the reference implementation — toolbar 48 px, inspector strip 40 px, RADIUS_* already token-derived. Gaps: dropdown popups, alerts, hover transitions, and its inline toast should adopt §8.14 anatomy. |
| Settings window (qs-app, SDF) | Currently hand-positioned pixels. Target: same anatomy via layout helpers (`row`, `section`, `card`) so the 7 pages share one rhythm. Nav = §8.11. |
| Overlay toolbar (HUD) | Always `hud` dark (floats over screenshots); icons = `qs-icons`; hover pill `hud_hover`; hit rects already SDF. Motion: armed-tool pill cross-fade. |
| Chrome capture cards | L4 cards; hover = whole-card `control_hot` lift + reveal × (delete) at 20 %→100 % in `FAST`; keep hysteresis. |
| Toast window | §8.14 — already close; align anatomy + TTLs. |
| Tray menu | Native menu — OS anatomy wins, don't custom-draw. |

## 10. Hard rules

1. No raw colour/space/radius/font/duration literals in draw code —
   token or a named constant derived from one (`RADIUS_*` pattern is fine).
2. Every interactive element: hover state + transition + ≥28 px hit target.
3. Lucide (`qs-icons`) is the only icon source; font-glyph badges and
   brand marks are the only exceptions.
4. New component? Add its anatomy to §8 first, then implement once per
   renderer — never a one-off style.
5. Accessibility: text roles must stay AA — `fg`/`fg_dim` on `surface`,
   `on_accent` on accent; `fg_faint` is decorative-only.
