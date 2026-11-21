# RDiskInfo

[![CI](https://github.com/yurinogueira/RDiskInfo/actions/workflows/ci.yml/badge.svg)](https://github.com/yurinogueira/RDiskInfo/actions/workflows/ci.yml)

A Rust S.M.A.R.T. and hardware disk inspector for Linux, inspired by **CrystalDiskInfo** and **QDiskInfo**.

## Features

- **Cross-protocol S.M.A.R.T. & Health Analysis**: Full support for **ATA / SATA** (HDD & SSD), **NVMe** (PCIe SSDs), and **SCSI / SAS / USB** bridges via `smartmontools` (`smartctl`).
- **Complete Technical Telemetry**:
  - Health status, remaining SSD lifespan %, current/min/max/threshold temperatures, and multi-sensor `hwmon` readouts.
  - Firmware, masked/revealable Serial Number (with copy button), Interface, Transfer Mode (SATA link speed / PCIe Gen + lane width), Standard, WWN, Form Factor, Sector Sizes, and supported Features (`S.M.A.R.T.`, `TRIM`, `VolatileWriteCache`, `NCQ`, `Self-Test`, etc.).
  - Total Host Reads / Writes, Rotation Rate, Power On Count, Power On Hours, Mount Points, Partitions, Filesystem Usage bars (including RAID/LVM/LUKS children), PCIe BDF & NUMA info, Kernel I/O Scheduler & Queue parameters, Live Block I/O stats, ATA Device Statistics, SATA PHY counters, NVMe Namespaces & Power States.
- **Diagnostics & Tools**:
  - Trigger & abort S.M.A.R.T. Self-Tests (`Short`, `Conveyance`, `Extended`) and inspect Self-Test & Error logs.
  - **ASCII / Hex Raw View** for both **ATA/SCSI (`SG_IO`)** and **NVMe (`NVME_IOCTL_ADMIN_CMD`)** with `.bin` export.
  - **Grid View** with instant disk search and SVG status indicators.
  - Export full reports to **JSON**, **TXT**, **Clipboard**, or **PNG Screenshot**.
- **Automatic Privilege Elevation**:
  - Probes unprivileged access first; if root privileges are required for S.M.A.R.T. devices, automatically prompts once via `pkexec` using an isolated `--privileged-helper` subprocess and still exposes `sysfs` / `lsblk` / `hwmon` data gracefully if elevation is declined.
- **Themes & Personalization**:
  - 6 built-in themes: `Native Light`, `Modern Dark`, `Crystal Classic Blue`, `Sakura Aoi (Stylized Light)`, `Sakura Night (Stylized Dark)`, `Cyber Midnight`.
  - Custom background wallpaper loader + overlay opacity slider.
- **Multi-Language UI**:
  - Automatically detects the OS locale (`LANG` / `LC_ALL` / `LC_MESSAGES`) with live switching between English, Português (Brasil), Español, Français, Deutsch, Italiano, 日本語, and 简体中文.

## Requirements

- Linux with `smartmontools` (`smartctl`), `lsblk` (`util-linux`), and `pkexec` (`polkit`) for automatic privilege elevation.

## Usage

```sh
cargo run --release
```

CLI options:

```sh
rdiskinfo --cli           # Print full text report to stdout
rdiskinfo --json          # Print JSON report to stdout
rdiskinfo --no-elevate    # Skip automatic pkexec elevation prompt
rdiskinfo --theme sakura  # Launch with a specific theme
rdiskinfo --lang pt-br    # Launch with a specific UI language
```
