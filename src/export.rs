use crate::config::AppConfig;
use crate::i18n::Strings;
use crate::model::{
    DiskInfo, DiskProtocol, format_capacity, format_data_volume, format_temperature,
};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

pub fn format_single_disk_report(disk: &DiskInfo, cfg: &AppConfig, tr: &Strings) -> String {
    let mut out = String::new();
    let cap = format_capacity(disk.capacity_bytes, cfg.use_gb_instead_of_tb);
    let _ = writeln!(
        out,
        "----------------------------------------------------------------------------"
    );
    let _ = writeln!(out, " {} : {}", disk.display_title(), cap);
    let _ = writeln!(
        out,
        "----------------------------------------------------------------------------"
    );
    let _ = writeln!(out, "{:>22} : {}", tr.export_model, disk.model_name);
    if let Some(fam) = &disk.model_family {
        let _ = writeln!(out, "{:>22} : {}", tr.export_model_family, fam);
    }
    let _ = writeln!(out, "{:>22} : {}", tr.firmware, disk.firmware_version);
    let serial_display = if cfg.hide_serial_by_default && !disk.serial_number.is_empty() {
        "(masked)".to_string()
    } else {
        disk.serial_number.clone()
    };
    let _ = writeln!(out, "{:>22} : {}", tr.serial_number, serial_display);
    let _ = writeln!(
        out,
        "{:>22} : {} ({} {})",
        tr.export_disk_size, cap, disk.capacity_bytes, tr.bytes_unit
    );
    let _ = writeln!(out, "{:>22} : {}", tr.interface_label, disk.interface_label);
    let _ = writeln!(out, "{:>22} : {}", tr.transfer_mode, disk.transfer_mode);
    let proto_val = if disk.device_type.is_empty() {
        disk.protocol_str.clone()
    } else {
        format!("{} ({})", disk.protocol_str, disk.device_type)
    };
    let _ = writeln!(out, "{:>22} : {}", tr.protocol, proto_val);
    let _ = writeln!(out, "{:>22} : {}", tr.device_node, disk.device_node);
    if let Some(blk) = &disk.block_device_node {
        let _ = writeln!(out, "{:>22} : {}", tr.export_block_node, blk);
    }
    let mounts = disk.all_mountpoints();
    if !mounts.is_empty() {
        let _ = writeln!(out, "{:>22} : {}", tr.drive_mounts, mounts.join(", "));
    }
    let _ = writeln!(out, "{:>22} : {}", tr.standard, disk.standard);
    if !disk.features.is_empty() {
        let _ = writeln!(out, "{:>22} : {}", tr.features, disk.features.join(", "));
    }
    let _ = writeln!(
        out,
        "{:>22} : {}",
        tr.total_reads,
        format_data_volume(disk.total_host_reads_bytes, cfg.use_gb_instead_of_tb)
    );
    let _ = writeln!(
        out,
        "{:>22} : {}",
        tr.total_writes,
        format_data_volume(disk.total_host_writes_bytes, cfg.use_gb_instead_of_tb)
    );
    if disk.total_nand_writes_bytes.is_some() {
        let _ = writeln!(
            out,
            "{:>22} : {}",
            tr.nand_writes,
            format_data_volume(disk.total_nand_writes_bytes, cfg.use_gb_instead_of_tb)
        );
    }
    let rot = match disk.rotation_rate {
        Some(0) => "---- (SSD)".to_string(),
        Some(rpm) => format!("{} RPM", rpm),
        None if disk.sysfs.rotational == Some(true) => "---- (HDD)".to_string(),
        None => "----".to_string(),
    };
    let _ = writeln!(out, "{:>22} : {}", tr.rotation_rate, rot);
    let _ = writeln!(
        out,
        "{:>22} : {}",
        tr.power_on_count,
        disk.power_cycle_count
            .map(|c| format!("{} {}", c, tr.count_unit))
            .unwrap_or_else(|| tr.unknown.to_string())
    );
    let hours_display = match (disk.power_on_hours, disk.power_on_minutes) {
        (Some(h), Some(m)) if m > 0 => format!("{} {} ({}m)", h, tr.hours_unit, m),
        (Some(h), _) => format!("{} {}", h, tr.hours_unit),
        _ => tr.unknown.to_string(),
    };
    let _ = writeln!(out, "{:>22} : {}", tr.power_on_hours, hours_display);
    if let Some(us) = disk.unsafe_shutdowns {
        let _ = writeln!(out, "{:>22} : {}", tr.unsafe_shutdowns, us);
    }
    if let Some(wwn) = &disk.wwn {
        let _ = writeln!(out, "{:>22} : {}", "WWN", wwn);
    }
    if let Some(bdf) = &disk.sysfs.pci_bdf {
        let _ = writeln!(out, "{:>22} : {}", tr.lbl_pci_slot, bdf);
    }
    if let Some(apm) = &disk.ata_apm {
        let state = if apm.enabled {
            apm.level
                .map(|l| format!("{} ({})", tr.val_enabled, l))
                .unwrap_or_else(|| tr.val_enabled.to_string())
        } else {
            tr.val_disabled.to_string()
        };
        let _ = writeln!(out, "{:>22} : {}", tr.lbl_ata_apm, state);
    }
    if let Some(aam) = &disk.ata_aam {
        let state = if aam.enabled {
            match (aam.level, aam.recommended_level) {
                (Some(l), Some(r)) => {
                    format!("{} ({}, {}: {})", tr.val_enabled, l, tr.lbl_recommended, r)
                }
                (Some(l), None) => format!("{} ({})", tr.val_enabled, l),
                _ => tr.val_enabled.to_string(),
            }
        } else {
            tr.val_disabled.to_string()
        };
        let _ = writeln!(out, "{:>22} : {}", tr.lbl_ata_aam, state);
    }
    if let Some(sec) = &disk.ata_security {
        let state = sec.state_string.clone().unwrap_or_else(|| {
            if sec.enabled {
                tr.val_enabled.to_string()
            } else {
                tr.val_disabled.to_string()
            }
        });
        let _ = writeln!(out, "{:>22} : {}", tr.lbl_ata_security, state);
    }
    let health_pct = disk
        .health_percentage
        .map(|p| format!(" ({}%)", p))
        .unwrap_or_default();
    let _ = writeln!(
        out,
        "{:>22} : {}{}",
        tr.health_status,
        tr.health_label(disk.health_status),
        health_pct
    );
    let _ = writeln!(
        out,
        "{:>22} : {}",
        tr.temperature,
        format_temperature(disk.temperature.current_celsius, cfg.use_fahrenheit)
    );

    if !disk.health_reasons.is_empty() {
        let _ = writeln!(
            out,
            "{:>22} : {}",
            tr.export_health_reasons,
            disk.health_reasons.join("; ")
        );
    }

    match disk.protocol {
        DiskProtocol::Ata if !disk.ata_attributes.is_empty() => {
            let _ = writeln!(out);
            let _ = writeln!(
                out,
                "-- S.M.A.R.T. --------------------------------------------------------------"
            );
            let _ = writeln!(
                out,
                "{:<2} {:>3} {:>3} {:>3} {:<12}  {}",
                tr.col_id,
                tr.col_current,
                tr.col_worst,
                tr.col_threshold,
                tr.col_raw_values,
                tr.col_attribute_name
            );
            for a in &disk.ata_attributes {
                let _ = writeln!(
                    out,
                    "{:02X} {:>3} {:>3} {:>3} {:012X}  {}",
                    a.id,
                    a.value,
                    a.worst,
                    a.thresh,
                    a.raw_value & 0xFFFF_FFFF_FFFF,
                    a.name
                );
            }
        }
        DiskProtocol::Nvme if !disk.nvme_attributes.is_empty() => {
            let _ = writeln!(out);
            let _ = writeln!(
                out,
                "-- S.M.A.R.T. (NVMe) -------------------------------------------------------"
            );
            let _ = writeln!(
                out,
                "{:<2} {:<16} {:<16} {}",
                tr.col_id, tr.col_raw_values, tr.col_decoded, tr.col_attribute_name
            );
            for a in &disk.nvme_attributes {
                let decoded = if a.key == "temperature" || a.key.starts_with("temperature_sensor_")
                {
                    let c = (a.raw_value as i64 - 273) as i32;
                    format_temperature(Some(c), cfg.use_fahrenheit)
                } else if a.key == "data_units_read" || a.key == "data_units_written" {
                    format_data_volume(Some(a.raw_value * 512_000), cfg.use_gb_instead_of_tb)
                } else {
                    a.decoded_value.clone()
                };
                let _ = writeln!(
                    out,
                    "{:02X} {:016X} {:<16} {}",
                    a.id, a.raw_value, decoded, a.name
                );
            }
        }
        DiskProtocol::Scsi if !disk.scsi_error_log.is_empty() => {
            let _ = writeln!(out);
            let _ = writeln!(out, "-- {} --", tr.export_scsi_error_log);
            for r in &disk.scsi_error_log {
                let _ = writeln!(
                    out,
                    "{:<8} {}={} {}={} {}={} {}={}",
                    r.operation,
                    tr.scsi_col_total_corrected.replace('\n', " "),
                    r.total_corrected,
                    tr.scsi_col_algo_invocations.replace('\n', " "),
                    r.algorithm_invocations,
                    tr.scsi_col_gb_processed.replace('\n', " "),
                    r.gigabytes_processed,
                    tr.scsi_col_total_uncorrected.replace('\n', " "),
                    r.total_uncorrected
                );
            }
        }
        _ => {}
    }

    out
}

pub fn format_full_report(disks: &[DiskInfo], cfg: &AppConfig, tr: &Strings) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "============================================================================"
    );
    let _ = writeln!(
        out,
        "{} v{}",
        tr.export_report_title,
        env!("CARGO_PKG_VERSION")
    );
    let _ = writeln!(
        out,
        "============================================================================"
    );
    let _ = writeln!(out, "\n-- {} ({}) --", tr.export_disk_list, disks.len());
    for (idx, d) in disks.iter().enumerate() {
        let cap = format_capacity(d.capacity_bytes, cfg.use_gb_instead_of_tb);
        let temp = format_temperature(d.temperature.current_celsius, cfg.use_fahrenheit);
        let _ = writeln!(
            out,
            " ({}) {} : {} [{}] ({}, {})",
            idx + 1,
            d.display_title(),
            cap,
            d.device_node,
            tr.health_label(d.health_status),
            temp
        );
    }
    let _ = writeln!(out);
    for d in disks {
        out.push_str(&format_single_disk_report(d, cfg, tr));
        out.push('\n');
    }
    out
}

pub fn export_disk_json(disk: &DiskInfo, path: &Path) -> Result<(), String> {
    let content = if let Some(raw) = &disk.raw_smartctl_json {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(raw) {
            serde_json::to_string_pretty(&val).unwrap_or_else(|_| raw.clone())
        } else {
            raw.clone()
        }
    } else {
        serde_json::to_string_pretty(disk).map_err(|e| e.to_string())?
    };
    fs::write(path, content).map_err(|e| e.to_string())
}

pub fn save_screenshot_png(image: &egui::ColorImage, path: &Path) -> Result<(), String> {
    let width = image.width() as u32;
    let height = image.height() as u32;
    let mut rgba = Vec::with_capacity(image.pixels.len() * 4);
    for px in &image.pixels {
        rgba.extend_from_slice(&px.to_array());
    }
    image::save_buffer(path, &rgba, width, height, image::ColorType::Rgba8)
        .map_err(|e| e.to_string())
}
