# UI, Theme & Localization Checklist (RDiskInfo)

## Supported Languages (`src/i18n.rs`)
- `Auto` (detects `LC_ALL` -> `LC_MESSAGES` -> `LANG` -> `LANGUAGE`, splitting on `:`, falling back to `En`)
- `En`, `PtBr`, `Es`, `Fr`, `De`, `It`, `Ja`, `ZhCn`

## Supported Themes (`src/config.rs`, `src/theme.rs`, `src/ui/artwork.rs`)
- `NativeLight` (QDiskInfo-style light)
- `ModernDark`
- `CrystalBlue` (CrystalDiskInfo classic blue — default)
- `SakuraAoi` (stylized light + vector mascot panel when width >= 980px and no custom wallpaper)
- `SakuraNight` (stylized dark + vector mascot panel)
- `CyberMidnight`
- Custom wallpaper image (`png`/`jpg`/`jpeg`/`webp`) + `panel_opacity` slider (`0.55..=1.0`)

## Keyboard & Mouse Shortcuts (`src/ui/mod.rs`)
- `F5` / `Ctrl+R`: Refresh devices (respects `elevation_declined`)
- `Ctrl+T`: Save selected disk JSON
- `Ctrl+S`: Save window PNG screenshot (delayed capture)
- `Ctrl+G`: Toggle Grid View
- `Ctrl+Q`: Quit
- `Alt+Left` / `Alt+Right` & Mouse `Extra1` / `Extra2`: Previous / next disk (respects `cyclic_navigation`)

## Export Caveat
- `src/export.rs` currently keeps literal `"(masked)"` in TXT reports for backward compatibility with `test_themes_and_export_report`; if localizing it later, update that test accordingly.
