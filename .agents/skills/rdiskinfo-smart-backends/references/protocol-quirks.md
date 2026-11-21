# Protocol & Kernel Quirks Reference (RDiskInfo)

## NVMe Invariants
- **Composite Temperature (`02`) & `temperature_sensor_1..8`**: `smartctl` JSON reports temperatures in Celsius, while NVMe spec raw values are in Kelvin. Store `(c + 273) as u128` in `NvmeSmartAttribute::raw_value` for BOTH Composite Temperature and Sensors 1..8, and decode `(raw_value as i64 - 273)` live in `src/ui/smart_table.rs` and `src/export.rs`.
- **Data Units**: Always `raw_value * 512_000` bytes regardless of 4Kn logical block size (per NVMe specification).
- **Available Spare Edge Cases**: Ignore `available_spare_threshold > 100` and `(spare == 0 && thresh == 0)`; `spare < thresh` -> `Bad`; `thresh != 100 && spare == thresh` -> `Caution`.
- **Power State Lines**: Handle optional `*` active-state token before `+`/`-` (`tokens` may shift by 1); operational flag is `+` vs `-` (do not use substring `" - "`).
- **Controller Names**: `nvme0n1`, `nvme0c1n1` -> `nvme0`; `nvme10n1` -> `nvme10` (`nvme_controller_name` parses leading digits after `"nvme"`).

## ATA / SATA Invariants
- **`when_failed`**: `"now"` -> `Bad`; `"in_the_past"` (or any non-empty value other than `"-"`) -> `Caution` with `(failed: <value>)`, never misleading `(value < thresh)` when `value >= thresh`.
- **Hardware Thermal Limits**: Store `op_limit_max` / `limit_max` (or NVMe WCTEMP/CCTEMP, SCSI `drive_trip`, `hwmon` `max`/`crit`) in `drive_warning_threshold` / `drive_critical_threshold` so `evaluate_health` preserves them across Settings edits.
- **Unreported `0 °C`**: Treat `<= 0` or `>= 300` from smartctl/hwmon as unreported (`None`) so `format_temperature` and `temp_status` stay consistent (`-- °C` + `Unknown`).
- **Extra ATA Features**: Parse `ata_apm`, `ata_aam`, `ata_security` (with `state` bitmask fallbacks `0x02`/`0x04`/`0x08`/`0x10`), `ata_sct_capabilities` / `ata_sct_status` (both object and plain string `device_state`) / `ata_sct_temperature_history`, and `ata_smart_data.self_test.status` (prefer `remaining_percent`, fallback to low nibble `(value & 0x0f) * 10`, then text regex/parse).
- **Brand-new drives (`power_on_hours == 0`)**: Guard any heuristic division/ratio with `power_on_hours > 0`.

## RAID & Multi-Target Devices
- `is_raid_controller_dev_type` splits on `'+'` to recognize composite types like `sat+megaraid,0`, `scsi+cciss,1`, `3ware,0`, `areca,1`, `hpt,1/1`.
- `collect_scan_targets` deduplicates by `(name, dev_type)` and skips `/dev/bus/*` in `extra_nodes` so bare `/dev/bus/0` is never probed without `-d`.
- Virtual devices filtered in `src/sysfs.rs`: `zram*`, `loop*`, `ram*`, `dm-*`, `md*`, `sr*`, `nbd*`, `fd[0-9]+`, `mmcblk*boot*`, `mmcblk*rpmb*`, and `type` `rom`/`loop`.

## `pkexec` Privileged Helper Protocol (`--privileged-helper`)
- JSONL over stdin/stdout (`HelperRequest` / `HelperResponse` in `src/privilege.rs`).
- On `HelperClient::spawn()` failure, close `stdin` first, poll `try_wait()` briefly before `kill()`, and read `stderr` so polkit exit codes `126`/`127` map to `ElevationState::Denied`.
- On `HelperClient::drop()`, send `Shutdown` only if `self.alive`, do a bounded short `try_wait()` loop, then `kill()` + `wait()` — never block indefinitely on dead pipes.
