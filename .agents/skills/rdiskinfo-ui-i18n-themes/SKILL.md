---
name: rdiskinfo-ui-i18n-themes
description: >-
  Guides editing or adding UI views, dialogs, themes, artwork, keyboard shortcuts,
  and translations in RDiskInfo's eframe/egui frontend and TXT exporter. Use when
  adding UI widgets, new tabs, theme presets, language strings, settings controls,
  or fixing layout, contrast, font glyph, or localization issues.
---

# RDiskInfo UI, Themes & Multi-Language Guide

Follow these steps whenever modifying `src/ui/*`, `src/theme.rs`, `src/config.rs`,
`src/i18n.rs`, or `src/export.rs`. For the full verification checklist, see
[ui-i18n-checklist.md](./references/ui-i18n-checklist.md).

## Workflow

1. **Add localized strings first (`src/i18n.rs`)**:
   - Add every new UI/tooltip/dialog/toast/report label to `Strings`.
   - Translate it in all 8 static catalogs: `EN`, `PT_BR`, `ES`, `FR`, `DE`, `IT`, `JA`, `ZH_CN`.
   - Append the new field to `Strings::all_field_values()` so `test_all_eight_languages_complete_and_localized_txt_reports` verifies non-empty coverage automatically.
   - Keep `parse_locale_string` handling colon-separated `LANGUAGE` lists (`xx_YY:pt_BR.UTF-8:en`).
2. **Format units dynamically at render/export time**:
   - Compute `°C`/`°F` and `TB`/`GB` live from raw values using `cfg.use_fahrenheit` and `cfg.use_gb_instead_of_tb` inside `smart_table.rs`, `summary_panel.rs`, `details_tabs.rs`, and `export.rs` so toggling Settings updates immediately without rescanning.
3. **Preserve Linux `egui` glyph & layout invariants**:
   - Keep `egui = { features = ["monochrome_emoji_fonts"] }` in `Cargo.toml`.
   - If introducing any new Unicode symbol/icon, add it to the glyph assertion loop in `test_egui_glyph_coverage_and_compact_window_render`.
   - Use `.truncate()` + `.on_hover_text(...)` on narrow spec cells; place trailing row buttons FIRST in `Layout::right_to_left` so they cannot be pushed off-screen at `720x520`.
   - Wrap wide tables in `ScrollArea::horizontal()` and pass `&mut ro_ref` (`&str`) to read-only `TextEdit::multiline`.
   - Avoid attaching empty `.on_hover_text("")` tooltips.
4. **Preserve theme, contrast & modal invariants**:
   - Only call `palette.apply_to_ctx(&ctx)` when `(config.theme, opacity_key)` changes (`applied_theme_state`).
   - Render status badges using `palette.status_colors(status)` pill `(bg, fg, orb)` rather than raw yellow/bright text on light themes.
   - Keep screenshot capture (`Ctrl+S` / `Save (Image)`) delayed by 2 frames (`pending_screenshot`) so open menus vanish before `ctx.request_screenshot(...)`.
   - Disambiguate duplicate `device_node` drives (RAID) via `disk_filename_stem` and `[device_type]` labels, and include `(&disk.device_node, &disk.device_type)` in `egui` scroll/table/grid `id_salt`s.
   - Clamp temperature thresholds via `AppConfig::set_ata_temp_thresholds` / `set_nvme_temp_thresholds` (`warning < critical`) and call `reevaluate_all_disks()` + `config.save()`.
5. **Verify headless rendering & translations**:
   - Extend `test_egui_headless_render_all_themes_and_protocols` or `test_egui_headless_new_panels_and_threshold_modal` for new widgets/dialogs.
   - Run:
     ```sh
     cargo fmt --check
     cargo clippy --all-targets -- -D warnings
     cargo test
     ```
