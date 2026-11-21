---
name: rdiskinfo-release-qa
description: >-
  Runs the full RDiskInfo build, lint, test, desktop-entry validation, and safe
  CLI/JSON host smoke-test suite, plus provides the manual interactive root/GUI
  checklist and cross-platform porting architecture map. Use before finishing
  changes, cutting a release, or planning Windows/macOS support.
---

# RDiskInfo Release & QA Verification

Run this verification workflow from the repository root after code changes.

## 1. Automated Code Quality & Tests

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build
cargo build --release
```

Confirm zero warnings and zero failed/ignored tests.

## 2. Desktop Entry Validation

```sh
desktop-file-validate dist/rdiskinfo.desktop
```

Keep `Categories=System;Monitor;` so validation passes with zero hints or warnings.

## 3. Safe Non-Privileged Host Smoke Tests

Run these without triggering unwanted desktop `pkexec` dialogs:

```sh
./target/release/rdiskinfo --help
./target/release/rdiskinfo --cli --no-elevate --lang pt-br
./target/release/rdiskinfo --cli --no-elevate --lang en
./target/release/rdiskinfo --json --no-elevate | head -n 80
env -u DISPLAY -u WAYLAND_DISPLAY ./target/release/rdiskinfo --json < /dev/null | tail -n 20
```

Expected results:
- `--cli --no-elevate --lang pt-br` outputs localized Portuguese headers/fields and real physical drives (excluding `zram`/`loop`).
- Headless `--json < /dev/null` exits immediately (no hang) with `"elevation": {"Unavailable": ...}`.
- Running `--ascii-view /dev/sda` or `--privileged-helper` without root exits with code `1` and a clean error message.

## 4. Manual Interactive Desktop / Root Checklist

When testing interactively on a Linux desktop session:
1. Launch `./target/release/rdiskinfo`, authenticate `pkexec` once, and confirm SMART/NVMe attributes populate across all physical drives.
2. Cancel `pkexec` in a fresh run, press `F5`, and confirm it does NOT re-prompt until clicking **Unlock S.M.A.R.T.**.
3. Open **Device -> ASCII / Hex View** on both an ATA and NVMe drive and test saving `.bin`.
4. Open **Settings -> Temperature Thresholds**, adjust sliders (`warning < critical`), verify immediate color/badge re-evaluation and persistence across restarts.
5. Test `Ctrl+T` (JSON), `Ctrl+S` (PNG without menu artifacts), `Ctrl+G` (Grid View), theme switching (`Sakura Aoi`, `Modern Dark`, custom wallpaper), and language switching (`pt-BR`, `ja`, `zh-CN`).

## 5. Future Cross-Platform Porting Map (Windows / macOS)

Portable modules (no OS-specific changes required): `src/model.rs`, `src/smartctl.rs`, `src/theme.rs`, `src/i18n.rs`, `src/export.rs`, `src/ui/*`.
Linux-only modules to abstract behind `#[cfg(target_os = ...)]`:
- `src/sysfs.rs`: replace `/sys/block`, `hwmon`, `lsblk`, `libc::statvfs` with WMI/Win32 volume APIs (Windows) or IOKit/DiskArbitration (macOS).
- `src/privilege.rs`: replace `pkexec`, `geteuid`, `PermissionsExt` with UAC manifest/ShellExecuteW `runas` (Windows) or Authorization Services (macOS).
- `src/raw_ioctl.rs`: replace `SG_IO` / `NVME_IOCTL_ADMIN_CMD` with `DeviceIoControl` (`IOCTL_STORAGE_QUERY_PROPERTY`, `IOCTL_ATA_PASS_THROUGH`, `SMART_RCV_DRIVE_DATA`) on Windows or IOKit SMART interfaces on macOS.
- `Cargo.toml`: enable platform-appropriate `eframe` features via `[target.'cfg(...)'.dependencies]`.
