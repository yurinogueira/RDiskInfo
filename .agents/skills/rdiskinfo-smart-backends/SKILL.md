---
name: rdiskinfo-smart-backends
description: >-
  Guides extending or debugging RDiskInfo's Linux disk telemetry backends:
  smartctl JSON parsing (ATA/SATA, NVMe, SCSI/SAS), sysfs/lsblk/hwmon discovery,
  pkexec privileged helper IPC, RAID -d targets, and direct SG_IO / NVME_IOCTL_ADMIN_CMD
  raw binary reads. Use when adding disk fields, fixing SMART/thermal/health bugs,
  supporting new storage controllers, or changing privilege elevation.
---

# RDiskInfo Storage & S.M.A.R.T. Backends

Follow these procedures and invariants when modifying `src/model.rs`, `src/smartctl.rs`,
`src/sysfs.rs`, `src/privilege.rs`, or `src/raw_ioctl.rs`. For detailed protocol tables
and edge cases, read [protocol-quirks.md](./references/protocol-quirks.md).

## Workflow for Adding or Fixing Disk Telemetry

1. **Extend the model (`src/model.rs`)**:
   - Add typed, `Serialize`/`Deserialize` fields with `#[serde(default)]` where backward compatibility matters.
   - Keep raw hardware values separate from display strings (e.g., store bytes as `u128`, temperatures in Celsius/Kelvin, hardware thresholds in `drive_warning_threshold` / `drive_critical_threshold`).
2. **Parse `smartctl -x --json=o` (`src/smartctl.rs`)**:
   - Preserve RAID `fallback_type` when `smartctl` JSON downgrades `device.type` to a non-RAID type (e.g. `"sat"` after `-d megaraid,2`).
   - Leave `logical_block_size` / `physical_block_size` as `0` when absent in JSON so `merge_sysfs_and_partitions` fills real sysfs sector sizes (e.g. AF `4096`) before defaulting to `512`.
   - Re-evaluate health and thermal status idempotently inside `evaluate_health` (including `DiskProtocol::Unknown` for unprivileged/USB drives).
3. **Merge unprivileged `sysfs` / `lsblk` / `hwmon` (`src/sysfs.rs`, `src/smartctl.rs`)**:
   - Use `nvme_controller_name` and `find_bundle_for_smart_device` so `nvme1` never matches `nvme10n1`, multipath `nvme0c1n1` resolves to `nvme0`, and RAID `/dev/bus/N` targets never inherit unrelated block bundles.
   - Check nested `<device>/hwmon/hwmonN/` (`drivetemp`) and deduplicate sensors by normalized label (`Temperature Sensor N` vs `Sensor N`).
   - Check `<ancestor>/link*/ata_link/link*/sata_spd` before `/sys/class/ata_link/linkN/sata_spd`.
4. **Update `pkexec` helper IPC & self-tests (`src/privilege.rs`)**:
   - Always validate `device_path` (`validate_device_path`) and `dev_type` (`validate_dev_type`).
   - Forward `-d <dev_type>` (when non-empty and `!= "scsi"`) via `build_start_self_test_args` / `build_abort_self_test_args` and `collect_scan_targets`.
   - Treat `smartctl` exit codes as **bitmasks**: `(exit & 0x07) == 0` succeeds even when bit 6 (`64`, error log entries present) is set; invoke `smartctl --json=o -t` without `-x`.
   - Keep `can_prompt_elevation()` guarding headless/non-TTY runs and use `ElevationState::summary()` for user-facing errors.
5. **Update raw binary ioctls (`src/raw_ioctl.rs`)**:
   - Validate `SgIoHdr` via `sg_io_succeeded` (`rc == 0`, `status`, `host_status`, `driver_status`, `(info & 0x01) == 0`, `resid < dxfer_len`).
   - Try `build_ata_passthrough_12` (`0x08`, plus `0x0c` quirk for `0xB0`) with `build_ata_passthrough_16` (`0x85`) fallback.
   - Route canonicalized symlinks and `/dev/ng*` to NVMe ioctls and try NSIDs `[0xFFFF_FFFF, 0, 1]` (or `NVME_IOCTL_ID` for Identify Namespace).
6. **Add fixture tests & verify**:
   - Add JSON/sysfs/ioctl regression tests in `tests/fixtures_tests.rs`.
   - Run:
     ```sh
     cargo fmt --check
     cargo clippy --all-targets -- -D warnings
     cargo test
     env -u DISPLAY -u WAYLAND_DISPLAY cargo run -- --json < /dev/null | tail -n 15
     ```
