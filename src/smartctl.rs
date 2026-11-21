use crate::model::{
    AtaAamInfo, AtaApmInfo, AtaSctInfo, AtaSecurityInfo, DevStatEntry, DevStatPage, DiskInfo,
    DiskProtocol, ErrorLogEntry, HealthEvalOptions, HealthStatus, NvmeNamespaceInfo,
    NvmePowerStateInfo, NvmeSmartAttribute, PartitionInfo, PhyEventCounter, ScsiErrorCounterRow,
    SelfTestEntry, SelfTestExecutionStatus, SmartAttribute, SysfsInfo, TempSensor,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScannedDevice {
    pub name: String,
    pub info_name: String,
    pub dev_type: String,
    pub protocol: String,
}

pub fn parse_scan_json(json_str: &str) -> Vec<ScannedDevice> {
    let Ok(val) = serde_json::from_str::<Value>(json_str) else {
        return Vec::new();
    };
    let Some(arr) = val.get("devices").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|item| {
            let name = item.get("name")?.as_str()?.to_string();
            let info_name = item
                .get("info_name")
                .and_then(|v| v.as_str())
                .unwrap_or(&name)
                .to_string();
            let dev_type = item
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let protocol = item
                .get("protocol")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Some(ScannedDevice {
                name,
                info_name,
                dev_type,
                protocol,
            })
        })
        .collect()
}

/// Split concatenated JSON objects (e.g. `...\n}\n{\n...`) if ever combined.
pub fn split_json_documents(raw: &str) -> Vec<String> {
    let mut docs = Vec::new();
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escaped = false;
    let mut start: Option<usize> = None;

    for (idx, ch) in raw.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => {
                if depth == 0 {
                    start = Some(idx);
                }
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    if let Some(s) = start.take() {
                        docs.push(raw[s..=idx].to_string());
                    }
                } else if depth < 0 {
                    depth = 0;
                    start = None;
                }
            }
            _ => {}
        }
    }
    docs
}

pub fn parse_smartctl_device_json(
    json_str: &str,
    fallback_node: Option<&str>,
    fallback_type: Option<&str>,
    eval_opts: &HealthEvalOptions,
) -> Option<DiskInfo> {
    let v: Value = serde_json::from_str(json_str).ok()?;
    let mut disk = DiskInfo {
        raw_smartctl_json: Some(json_str.to_string()),
        ..Default::default()
    };

    if let Some(output_arr) = v
        .get("smartctl")
        .and_then(|s| s.get("output"))
        .and_then(|o| o.as_array())
    {
        disk.raw_smartctl_output = output_arr
            .iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect();
    }

    if let Some(msgs) = v
        .get("smartctl")
        .and_then(|s| s.get("messages"))
        .and_then(|m| m.as_array())
    {
        let err_msgs: Vec<String> = msgs
            .iter()
            .filter(|m| m.get("severity").and_then(|s| s.as_str()) == Some("error"))
            .filter_map(|m| m.get("string").and_then(|s| s.as_str()).map(str::to_string))
            .collect();
        if !err_msgs.is_empty() {
            disk.smartctl_error_message = Some(err_msgs.join("; "));
        }
    }

    let dev_obj = v.get("device");
    disk.device_node = dev_obj
        .and_then(|d| d.get("name"))
        .and_then(|s| s.as_str())
        .or(fallback_node)
        .unwrap_or("")
        .to_string();
    let json_type = dev_obj
        .and_then(|d| d.get("type"))
        .and_then(|s| s.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    disk.device_type = match (json_type, fallback_type) {
        (Some(jt), Some(ft))
            if crate::sysfs::is_raid_controller_dev_type(ft)
                && !crate::sysfs::is_raid_controller_dev_type(jt) =>
        {
            ft.to_string()
        }
        (Some(jt), _) => jt.to_string(),
        (None, Some(ft)) => ft.to_string(),
        (None, None) => String::new(),
    };
    disk.protocol_str = dev_obj
        .and_then(|d| d.get("protocol"))
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();

    disk.protocol = match disk.protocol_str.as_str() {
        "ATA" => DiskProtocol::Ata,
        "NVMe" => DiskProtocol::Nvme,
        "SCSI" => DiskProtocol::Scsi,
        _ => {
            if v.get("nvme_smart_health_information_log").is_some() || disk.device_type == "nvme" {
                DiskProtocol::Nvme
            } else if v.get("ata_smart_attributes").is_some() {
                DiskProtocol::Ata
            } else if v.get("scsi_error_counter_log").is_some()
                || v.get("scsi_model_name").is_some()
            {
                DiskProtocol::Scsi
            } else {
                DiskProtocol::Unknown
            }
        }
    };

    if disk.protocol_str.is_empty() {
        disk.protocol_str = match disk.protocol {
            DiskProtocol::Ata => "ATA".to_string(),
            DiskProtocol::Nvme => "NVMe".to_string(),
            DiskProtocol::Scsi => "SCSI".to_string(),
            DiskProtocol::Unknown => "Unknown".to_string(),
        };
    }

    // Check if smartctl actually opened the device and read real info
    let has_model = v.get("model_name").is_some()
        || v.get("scsi_model_name").is_some()
        || v.get("scsi_product").is_some()
        || v.get("ata_smart_attributes").is_some()
        || v.get("nvme_smart_health_information_log").is_some();
    disk.smartctl_available_data = has_model;

    disk.model_family = v
        .get("model_family")
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let scsi_fallback_model = || {
        let vendor = v
            .get("scsi_vendor")
            .and_then(|x| x.as_str())
            .map(str::trim)
            .unwrap_or("");
        let product = v
            .get("scsi_product")
            .and_then(|x| x.as_str())
            .map(str::trim)
            .unwrap_or("");
        let combined = format!("{} {}", vendor, product).trim().to_string();
        if combined.is_empty() {
            None
        } else {
            Some(combined)
        }
    };

    disk.model_name = match disk.protocol {
        DiskProtocol::Scsi => v
            .get("scsi_model_name")
            .or_else(|| v.get("model_name"))
            .and_then(|x| x.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(scsi_fallback_model)
            .unwrap_or_default(),
        _ => v
            .get("model_name")
            .or_else(|| v.get("scsi_model_name"))
            .and_then(|x| x.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(scsi_fallback_model)
            .unwrap_or_default(),
    };

    disk.serial_number = v
        .get("serial_number")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    disk.firmware_version = match disk.protocol {
        DiskProtocol::Scsi => v
            .get("scsi_revision")
            .or_else(|| v.get("firmware_version"))
            .and_then(|x| x.as_str())
            .unwrap_or("----")
            .trim()
            .to_string(),
        _ => v
            .get("firmware_version")
            .and_then(|x| x.as_str())
            .unwrap_or("----")
            .trim()
            .to_string(),
    };

    if let Some(wwn) = v.get("wwn")
        && let (Some(naa), Some(oui), Some(id)) = (
            wwn.get("naa").and_then(|x| x.as_u64()),
            wwn.get("oui").and_then(|x| x.as_u64()),
            wwn.get("id").and_then(|x| x.as_u64()),
        )
    {
        disk.wwn = Some(format!("{:x} {:06x} {:09x}", naa, oui, id));
    }

    disk.form_factor = v
        .get("form_factor")
        .and_then(|f| f.get("name"))
        .and_then(|n| n.as_str())
        .map(str::to_string);

    disk.capacity_bytes = v
        .get("user_capacity")
        .and_then(|c| c.get("bytes"))
        .and_then(json_to_u64)
        .unwrap_or(0);

    disk.logical_block_size = v
        .get("logical_block_size")
        .and_then(json_to_u64)
        .unwrap_or(0);
    disk.physical_block_size = v
        .get("physical_block_size")
        .and_then(json_to_u64)
        .unwrap_or(0);

    if let Some(rot) = v.get("rotation_rate").and_then(json_to_u64) {
        disk.rotation_rate = Some(rot as u32);
    } else if disk.protocol == DiskProtocol::Nvme {
        disk.rotation_rate = Some(0);
    }

    disk.smart_passed = v
        .get("smart_status")
        .and_then(|s| s.get("passed"))
        .and_then(|p| p.as_bool());

    disk.smart_supported = v
        .get("smart_support")
        .and_then(|s| s.get("available"))
        .and_then(|b| b.as_bool());
    disk.smart_enabled = v
        .get("smart_support")
        .and_then(|s| s.get("enabled"))
        .and_then(|b| b.as_bool());

    if let Some(pot) = v.get("power_on_time") {
        disk.power_on_hours = pot.get("hours").and_then(json_to_u64);
        disk.power_on_minutes = pot.get("minutes").and_then(json_to_u64);
    }
    disk.power_cycle_count = v.get("power_cycle_count").and_then(json_to_u64);

    // Temperature
    if let Some(temp) = v.get("temperature") {
        disk.temperature.current_celsius = temp
            .get("current")
            .and_then(json_to_i32)
            .filter(|&c| c > 0 && c < 300);
        disk.temperature.power_cycle_min = temp.get("power_cycle_min").and_then(json_to_i32);
        disk.temperature.power_cycle_max = temp.get("power_cycle_max").and_then(json_to_i32);
        disk.temperature.lifetime_min = temp.get("lifetime_min").and_then(json_to_i32);
        disk.temperature.lifetime_max = temp.get("lifetime_max").and_then(json_to_i32);
        disk.temperature.op_limit_min = temp.get("op_limit_min").and_then(json_to_i32);
        disk.temperature.op_limit_max = temp.get("op_limit_max").and_then(json_to_i32);
        disk.temperature.limit_min = temp.get("limit_min").and_then(json_to_i32);
        disk.temperature.limit_max = temp.get("limit_max").and_then(json_to_i32);
        if let Some(trip) = temp.get("drive_trip").and_then(json_to_i32)
            && trip > 0
        {
            disk.temperature.drive_critical_threshold = Some(trip);
            disk.temperature.critical_threshold = trip;
        }
    }

    match disk.protocol {
        DiskProtocol::Ata => parse_ata_sections(&v, &mut disk, eval_opts),
        DiskProtocol::Nvme => parse_nvme_sections(&v, &mut disk, eval_opts),
        DiskProtocol::Scsi => parse_scsi_sections(&v, &mut disk, eval_opts),
        DiskProtocol::Unknown => {}
    }

    evaluate_health(&mut disk, eval_opts);
    Some(disk)
}

fn parse_ata_sections(v: &Value, disk: &mut DiskInfo, eval_opts: &HealthEvalOptions) {
    disk.temperature.warning_threshold = eval_opts.temp_warning_ata;
    disk.temperature.critical_threshold = eval_opts.temp_critical_ata;
    if let Some(op_max) = disk.temperature.op_limit_max
        && op_max > 0
    {
        disk.temperature.drive_warning_threshold = Some(op_max);
        disk.temperature.warning_threshold = op_max;
    }
    if let Some(lim_max) = disk.temperature.limit_max
        && lim_max > 0
    {
        disk.temperature.drive_critical_threshold = Some(lim_max);
        disk.temperature.critical_threshold = lim_max;
    }

    // Interface / Transfer Mode / Standard / Features
    let is_usb = disk.device_type.contains("usb");
    let is_sata = v.get("sata_version").is_some() || disk.device_type == "sat";
    disk.interface_label = if is_usb {
        "USB (SAT)".to_string()
    } else if is_sata {
        "Serial ATA".to_string()
    } else {
        "ATA / PATA".to_string()
    };

    let max_speed = v
        .get("interface_speed")
        .and_then(|s| s.get("max"))
        .and_then(|m| m.get("string"))
        .and_then(|s| s.as_str());
    let cur_speed = v
        .get("interface_speed")
        .and_then(|s| s.get("current"))
        .and_then(|c| c.get("string"))
        .and_then(|s| s.as_str());
    disk.transfer_mode = match (cur_speed, max_speed) {
        (Some(c), Some(m)) => format!("{} | {}", format_sata_speed(c), format_sata_speed(m)),
        (None, Some(m)) => format!("---- | {}", format_sata_speed(m)),
        (Some(c), None) => format_sata_speed(c),
        (None, None) => "----".to_string(),
    };

    let ata_ver = v
        .get("ata_version")
        .and_then(|x| x.get("string"))
        .and_then(|s| s.as_str());
    let sata_ver = v
        .get("sata_version")
        .and_then(|x| x.get("string"))
        .and_then(|s| s.as_str());
    disk.standard = match (ata_ver, sata_ver) {
        (Some(a), Some(s)) => format!("{} | {}", a, s),
        (Some(a), None) => a.to_string(),
        (None, Some(s)) => s.to_string(),
        (None, None) => "ATA".to_string(),
    };

    let mut features = Vec::new();
    if disk.smart_supported == Some(true) || disk.smart_enabled == Some(true) {
        features.push("S.M.A.R.T.".to_string());
    }
    if v.get("trim")
        .and_then(|t| t.get("supported"))
        .and_then(|b| b.as_bool())
        == Some(true)
    {
        features.push("TRIM".to_string());
    }
    if v.get("write_cache")
        .and_then(|w| w.get("enabled"))
        .and_then(|b| b.as_bool())
        == Some(true)
    {
        features.push("VolatileWriteCache".to_string());
    } else if v
        .get("write_cache")
        .and_then(|w| w.get("supported"))
        .and_then(|b| b.as_bool())
        == Some(true)
    {
        features.push("WriteCache (Off)".to_string());
    }
    if v.get("read_lookahead")
        .and_then(|r| r.get("enabled"))
        .and_then(|b| b.as_bool())
        == Some(true)
    {
        features.push("ReadLookAhead".to_string());
    }
    if v.get("ata_sct_capabilities").is_some() {
        features.push("SCT".to_string());
    }
    if v.get("ata_device_statistics").is_some() {
        features.push("DevStat".to_string());
    }
    if v.get("sata_version").is_some() {
        features.push("NCQ".to_string());
    }
    if v.get("ata_apm").is_some() {
        features.push("APM".to_string());
    }
    if v.get("ata_aam").is_some() {
        features.push("AAM".to_string());
    }
    parse_ata_extra_features(v, disk);
    if disk.ata_security.as_ref().is_some_and(|s| s.enabled) {
        features.push("Security".to_string());
    }

    disk.features = features;

    // Attributes table
    let mut total_reads_bytes: Option<u128> = None;
    let mut total_writes_bytes: Option<u128> = None;
    let mut total_nand_writes_bytes: Option<u128> = None;
    let mut ssd_health_pct: Option<u8> = None;
    let lbs = disk.logical_block_size.max(512) as u128;
    let power_on_hours = disk.power_on_hours.unwrap_or(0);

    if let Some(table) = v
        .get("ata_smart_attributes")
        .and_then(|a| a.get("table"))
        .and_then(|t| t.as_array())
    {
        for item in table {
            let id = item.get("id").and_then(json_to_u64).unwrap_or(0).min(255) as u8;
            let name = item
                .get("name")
                .and_then(|s| s.as_str())
                .unwrap_or("Unknown")
                .replace('_', " ");
            let raw_name = item.get("name").and_then(|s| s.as_str()).unwrap_or("");
            let value = item
                .get("value")
                .and_then(json_to_u64)
                .unwrap_or(0)
                .min(255) as u8;
            let worst = item
                .get("worst")
                .and_then(json_to_u64)
                .unwrap_or(0)
                .min(255) as u8;
            let thresh = item
                .get("thresh")
                .and_then(json_to_u64)
                .unwrap_or(0)
                .min(255) as u8;
            let when_failed = item
                .get("when_failed")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();
            let flags = item.get("flags");
            let flags_short = flags
                .and_then(|f| f.get("string"))
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let prefailure = flags
                .and_then(|f| f.get("prefailure"))
                .and_then(|b| b.as_bool())
                .unwrap_or(false);
            let updated_online = flags
                .and_then(|f| f.get("updated_online"))
                .and_then(|b| b.as_bool())
                .unwrap_or(false);
            let raw_value = item
                .get("raw")
                .and_then(|r| r.get("value"))
                .and_then(json_to_u64)
                .unwrap_or(0);
            let raw_string = item
                .get("raw")
                .and_then(|r| r.get("string"))
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();

            // Health percentage extraction (matches QDiskInfo/CrystalDiskInfo heuristics)
            match (id, raw_name) {
                (202, "Percent_Lifetime_Remain") => {
                    let rem = 100i64.saturating_sub(raw_value as i64).clamp(0, 100) as u8;
                    ssd_health_pct = Some(rem);
                }
                (_, "Remaining_Lifetime_Perc") => {
                    ssd_health_pct = Some((raw_value.min(100)) as u8);
                }
                (_, "Percent_Lifetime_Remain") => {
                    ssd_health_pct = Some(value.min(100));
                }
                (_, "Media_Wearout_Indicator" | "SSD_Life_Left") if worst <= 100 => {
                    ssd_health_pct = Some(value.min(100));
                }
                _ => {}
            }

            // Host writes / reads extraction
            match id {
                241 => match raw_name {
                    "Total_Writes_GB" => {
                        total_writes_bytes = Some((raw_value as u128) * 1_000_000_000);
                    }
                    "Host_Writes_32MiB" | "Total_Writes_32MiB" => {
                        total_writes_bytes = Some((raw_value as u128) * 32 * 1024 * 1024);
                    }
                    "Host_Writes_MiB" | "Total_Writes_MB" => {
                        total_writes_bytes = Some((raw_value as u128) * 1024 * 1024);
                    }
                    "Host_Writes_GiB" | "Lifetime_Writes_GiB" | "Host_Writes_1GiB" => {
                        total_writes_bytes = Some((raw_value as u128) * (1u128 << 30));
                    }
                    "Total_LBAs_Written" | "Total_Host_Writes" => {
                        let mut bytes = (raw_value as u128) * lbs;
                        let mb = bytes / 1_000_000;
                        // Heuristic from QDiskInfo when drive reports 32MiB chunks under Total_LBAs_Written
                        if power_on_hours > 0 && (mb as u64) < power_on_hours && raw_value > 0 {
                            bytes = (raw_value as u128) * 32 * 1024 * 1024;
                        }
                        total_writes_bytes = Some(bytes);
                    }
                    _ => {}
                },
                242 => match raw_name {
                    "Total_Reads_GB" => {
                        total_reads_bytes = Some((raw_value as u128) * 1_000_000_000);
                    }
                    "Host_Reads_32MiB" | "Total_Reads_32MiB" => {
                        total_reads_bytes = Some((raw_value as u128) * 32 * 1024 * 1024);
                    }
                    "Host_Reads_MiB" | "Total_Reads_MB" => {
                        total_reads_bytes = Some((raw_value as u128) * 1024 * 1024);
                    }
                    "Host_Reads_GiB" | "Lifetime_Reads_GiB" | "Host_Reads_1GiB" => {
                        total_reads_bytes = Some((raw_value as u128) * (1u128 << 30));
                    }
                    "Total_LBAs_Read" | "Total_Host_Reads" => {
                        let mut bytes = (raw_value as u128) * lbs;
                        let mb = bytes / 1_000_000;
                        if power_on_hours > 0 && (mb as u64) < power_on_hours && raw_value > 0 {
                            bytes = (raw_value as u128) * 32 * 1024 * 1024;
                        }
                        total_reads_bytes = Some(bytes);
                    }
                    _ => {}
                },
                246 if raw_name == "Total_LBAs_Written" => {
                    total_writes_bytes = Some((raw_value as u128) * lbs);
                }
                249 if raw_name == "NAND_Writes_1GiB" => {
                    total_nand_writes_bytes = Some((raw_value as u128) * (1u128 << 30));
                }
                _ => {}
            }

            let note = decode_ata_attribute_note(id, raw_name, raw_value, &raw_string, prefailure);

            disk.ata_attributes.push(SmartAttribute {
                id,
                name,
                value,
                worst,
                thresh,
                when_failed,
                flags_short,
                prefailure,
                updated_online,
                raw_value,
                raw_string,
                status: HealthStatus::Good,
                note,
            });
        }
    }

    // ATA Device Statistics (overrides legacy read/write heuristics when present)
    if let Some(pages) = v
        .get("ata_device_statistics")
        .and_then(|d| d.get("pages"))
        .and_then(|p| p.as_array())
    {
        for p in pages {
            let page_number = p.get("number").and_then(json_to_u64).unwrap_or(0) as u8;
            let page_name = p
                .get("name")
                .and_then(|s| s.as_str())
                .unwrap_or("Statistics")
                .to_string();
            let mut entries = Vec::new();
            if let Some(tbl) = p.get("table").and_then(|t| t.as_array()) {
                for row in tbl {
                    let key = row
                        .get("name")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string();
                    let val_str = row.get("value").map(format_json_scalar).unwrap_or_default();

                    if page_name == "General Statistics" {
                        if key == "Logical Sectors Read" {
                            if let Some(sectors) = row.get("value").and_then(json_to_u128) {
                                total_reads_bytes = Some(sectors * lbs);
                            }
                        } else if key == "Logical Sectors Written"
                            && let Some(sectors) = row.get("value").and_then(json_to_u128)
                        {
                            total_writes_bytes = Some(sectors * lbs);
                        }
                    } else if key == "Percentage Used Endurance Indicator"
                        && ssd_health_pct.is_none()
                        && let Some(pct_used) = row.get("value").and_then(json_to_u64)
                    {
                        ssd_health_pct = Some(100u64.saturating_sub(pct_used).min(100) as u8);
                    }

                    entries.push(DevStatEntry {
                        name: key,
                        value: val_str,
                        raw_hex: row
                            .get("flags")
                            .and_then(|f| f.get("value"))
                            .and_then(json_to_u64)
                            .map(|x| format!("0x{:04X}", x)),
                    });
                }
            }
            disk.ata_device_stats.push(DevStatPage {
                page_number,
                page_name,
                entries,
            });
        }
    }

    if ssd_health_pct.is_none()
        && disk.rotation_rate == Some(0)
        && let Some(attr) = disk
            .ata_attributes
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case("Wear Leveling Count"))
    {
        ssd_health_pct = Some(attr.value.min(100));
    }

    disk.total_host_reads_bytes = total_reads_bytes;
    disk.total_host_writes_bytes = total_writes_bytes;
    disk.total_nand_writes_bytes = total_nand_writes_bytes;
    disk.health_percentage = ssd_health_pct;

    // Self-test polling minutes & current execution status
    if let Some(st) = v.get("ata_smart_data").and_then(|d| d.get("self_test")) {
        if let Some(pm) = st.get("polling_minutes").and_then(|o| o.as_object()) {
            let mut list: Vec<(String, u32)> = pm
                .iter()
                .filter_map(|(k, val)| Some((k.clone(), json_to_u64(val)? as u32)))
                .collect();
            list.sort_by_key(|(_, mins)| *mins);
            disk.self_test_polling_minutes = list;
        }
        if let Some(status_obj) = st.get("status") {
            let status_text = status_obj
                .get("string")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let raw_val = status_obj.get("value").and_then(json_to_u64);
            let rem_explicit = status_obj
                .get("remaining_percent")
                .and_then(json_to_u64)
                .map(|r| r.min(100) as u8);
            let nibble_in_progress = raw_val.is_some_and(|x| (x & 0xF0) == 0xF0);
            let in_progress = rem_explicit.is_some()
                || nibble_in_progress
                || status_text.to_ascii_lowercase().contains("in progress");
            let remaining_percent = if in_progress {
                rem_explicit
                    .or_else(|| {
                        raw_val
                            .filter(|x| (x & 0xF0) == 0xF0)
                            .map(|x| (((x & 0x0F) * 10).min(100)) as u8)
                    })
                    .or_else(|| extract_remaining_percent_from_text(&status_text))
            } else {
                None
            };
            let completed_percent = remaining_percent.map(|r| 100u8.saturating_sub(r));
            if !status_text.is_empty() || in_progress {
                disk.self_test_execution = Some(SelfTestExecutionStatus {
                    in_progress,
                    status_text,
                    remaining_percent,
                    completed_percent,
                });
            }
        }
    }

    // Self-test log (check extended log first, fallback to standard)
    let st_table = v
        .get("ata_smart_self_test_log")
        .and_then(|l| l.get("extended"))
        .and_then(|s| s.get("table"))
        .and_then(|t| t.as_array())
        .filter(|arr| !arr.is_empty())
        .or_else(|| {
            v.get("ata_smart_self_test_log")
                .and_then(|l| l.get("standard"))
                .and_then(|s| s.get("table"))
                .and_then(|t| t.as_array())
        });
    if let Some(tbl) = st_table {
        for entry in tbl {
            disk.self_test_log.push(SelfTestEntry {
                num: entry.get("num").and_then(json_to_u64).map(|x| x as u32),
                test_type: entry
                    .get("type")
                    .and_then(|t| t.get("string"))
                    .and_then(|s| s.as_str())
                    .unwrap_or("Unknown")
                    .to_string(),
                status: entry
                    .get("status")
                    .and_then(|t| t.get("string"))
                    .and_then(|s| s.as_str())
                    .unwrap_or("Unknown")
                    .to_string(),
                lifetime_hours: entry.get("lifetime_hours").and_then(json_to_u64),
                lba: entry
                    .get("lba_of_first_error")
                    .and_then(json_to_u64)
                    .map(|l| format!("0x{:X}", l)),
            });
        }
    }

    // Error log (check extended summary first, fallback to standard summary)
    let err_summary_opt = v
        .get("ata_smart_extended_error_log")
        .and_then(|e| e.get("summary"))
        .or_else(|| v.get("ata_smart_error_log").and_then(|e| e.get("summary")));
    if let Some(err_summary) = err_summary_opt {
        disk.error_log_count = err_summary.get("count").and_then(json_to_u64);
        if let Some(tbl) = err_summary.get("table").and_then(|t| t.as_array()) {
            for e in tbl {
                disk.error_logs.push(ErrorLogEntry {
                    error_number: e.get("error_number").and_then(json_to_u64),
                    lifetime_hours: e.get("lifetime_hours").and_then(json_to_u64),
                    state_or_status: e
                        .get("device_state")
                        .and_then(|d| d.get("string"))
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string(),
                    description: e
                        .get("error_description")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string(),
                });
            }
        }
    }

    // SATA PHY event counters
    if let Some(tbl) = v
        .get("sata_phy_event_counters")
        .and_then(|p| p.get("table"))
        .and_then(|t| t.as_array())
    {
        for item in tbl {
            disk.sata_phy_counters.push(PhyEventCounter {
                id: item.get("id").and_then(json_to_u64).unwrap_or(0) as u16,
                name: item
                    .get("name")
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_string(),
                size: item.get("size").and_then(json_to_u64).unwrap_or(0) as u16,
                value: item.get("value").and_then(json_to_u64).unwrap_or(0),
                overflow: item
                    .get("overflow")
                    .and_then(|b| b.as_bool())
                    .unwrap_or(false),
            });
        }
    }
}

fn parse_nvme_sections(v: &Value, disk: &mut DiskInfo, eval_opts: &HealthEvalOptions) {
    disk.interface_label = "NVM Express".to_string();
    disk.temperature.warning_threshold = eval_opts.temp_warning_nvme;
    disk.temperature.critical_threshold = eval_opts.temp_critical_nvme;

    let nvme_ver = v
        .get("nvme_version")
        .and_then(|x| x.get("string"))
        .and_then(|s| s.as_str());
    disk.standard = match nvme_ver {
        Some(ver) if ver.starts_with("NVMe") => {
            format!("NVM Express {}", ver.trim_start_matches("NVMe").trim())
        }
        Some(ver) => format!("NVM Express {}", ver),
        None => "NVM Express".to_string(),
    };

    disk.nvme_pci_vendor_id = v
        .get("nvme_pci_vendor")
        .and_then(|x| x.get("id"))
        .and_then(json_to_u64)
        .map(|x| x as u16);
    disk.nvme_ieee_oui = v
        .get("nvme_ieee_oui_identifier")
        .and_then(json_to_u64)
        .map(|x| x as u32);
    disk.nvme_controller_id = v
        .get("nvme_controller_id")
        .and_then(json_to_u64)
        .map(|x| x as u32);
    disk.nvme_total_capacity_bytes = v.get("nvme_total_capacity").and_then(json_to_u128);
    disk.nvme_unallocated_capacity_bytes =
        v.get("nvme_unallocated_capacity").and_then(json_to_u128);

    if let Some(nss) = v.get("nvme_namespaces").and_then(|n| n.as_array()) {
        for ns in nss {
            disk.nvme_namespaces.push(NvmeNamespaceInfo {
                id: ns.get("id").and_then(json_to_u64).unwrap_or(1) as u32,
                size_bytes: ns
                    .get("size")
                    .and_then(|s| s.get("bytes"))
                    .and_then(json_to_u64),
                capacity_bytes: ns
                    .get("capacity")
                    .and_then(|s| s.get("bytes"))
                    .and_then(json_to_u64),
                utilization_bytes: ns
                    .get("utilization")
                    .and_then(|s| s.get("bytes"))
                    .and_then(json_to_u64),
                formatted_lba_size: ns
                    .get("formatted_lba_size")
                    .and_then(json_to_u64)
                    .map(|x| x as u32),
                eui64: ns.get("eui64").and_then(|e| {
                    let oui = e.get("oui")?.as_u64()?;
                    let ext = e.get("ext_id")?.as_u64()?;
                    Some(format!("{:06X}{:010X}", oui, ext))
                }),
                nguid: ns.get("nguid").and_then(|s| s.as_str()).map(str::to_string),
            });
        }
    }

    // Parse smartctl text output lines for optional commands, temp thresholds, power states
    let mut features = vec!["S.M.A.R.T.".to_string()];
    let mut in_power_states = false;
    for line in &disk.raw_smartctl_output {
        let trimmed = line.trim();
        if line.starts_with("Optional Admin Commands") {
            if line.contains("Self_Test") {
                disk.nvme_self_test_supported = true;
                features.push("Self-Test".to_string());
            }
            if line.contains("Format_NVM") {
                features.push("FormatNVM".to_string());
            }
            if line.contains("Firmware_DL") {
                features.push("FirmwareDL".to_string());
            }
        } else if line.starts_with("Optional NVM Commands") {
            if line.contains("Dataset_Mgmt") {
                features.push("TRIM".to_string());
            }
            if line.contains("Write_Zeroes") {
                features.push("WriteZeroes".to_string());
            }
        } else if line.contains("Volatile Write Cache") {
            if line.contains("Present") {
                features.push("VolatileWriteCache".to_string());
            }
        } else if line.starts_with("Warning  Comp. Temp. Threshold")
            || line.starts_with("Warning Comp. Temp. Threshold")
        {
            if let Some(t) = extract_temp_from_threshold_line(line) {
                disk.temperature.drive_warning_threshold = Some(t);
                disk.temperature.warning_threshold = t;
            }
        } else if line.starts_with("Critical Comp. Temp. Threshold") {
            if let Some(t) = extract_temp_from_threshold_line(line) {
                disk.temperature.drive_critical_threshold = Some(t);
                disk.temperature.critical_threshold = t;
            }
        } else if trimmed.starts_with("Supported Power States") {
            in_power_states = true;
        } else if in_power_states {
            if trimmed.is_empty() {
                in_power_states = false;
            } else if !trimmed.starts_with("St ")
                && let Some(ps) = parse_nvme_power_state_line(trimmed)
            {
                disk.nvme_power_states.push(ps);
            }
        }
    }
    parse_ata_extra_features(v, disk);

    disk.features = features;

    // Canonical NVMe SMART/Health attributes
    if let Some(log) = v
        .get("nvme_smart_health_information_log")
        .and_then(|l| l.as_object())
    {
        let canonical_keys: &[(&str, &str)] = &[
            ("critical_warning", "Critical Warning"),
            ("temperature", "Composite Temperature"),
            ("available_spare", "Available Spare"),
            ("available_spare_threshold", "Available Spare Threshold"),
            ("percentage_used", "Percentage Used"),
            ("data_units_read", "Data Units Read"),
            ("data_units_written", "Data Units Written"),
            ("host_reads", "Host Read Commands"),
            ("host_writes", "Host Write Commands"),
            ("controller_busy_time", "Controller Busy Time"),
            ("power_cycles", "Power Cycles"),
            ("power_on_hours", "Power On Hours"),
            ("unsafe_shutdowns", "Unsafe Shutdowns"),
            ("media_errors", "Media and Data Integrity Errors"),
            (
                "num_err_log_entries",
                "Number of Error Information Log Entries",
            ),
            ("warning_temp_time", "Warning Composite Temperature Time"),
            ("critical_comp_time", "Critical Composite Temperature Time"),
        ];

        let mut id_counter: u8 = 1;
        let mut seen = std::collections::HashSet::new();

        for (key, display) in canonical_keys {
            if *key == "temperature" {
                if let Some(c) = log.get("temperature").and_then(json_to_i32) {
                    seen.insert(*key);
                    let kelvin = (c as i64 + 273).max(0) as u128;
                    let attr = build_nvme_attr(id_counter, key, display, kelvin);
                    if disk.temperature.current_celsius.is_none() && c > 0 && c < 300 {
                        disk.temperature.current_celsius = Some(c);
                    }
                    disk.nvme_attributes.push(attr);
                    id_counter = id_counter.saturating_add(1);
                }
            } else if let Some(val) = log.get(*key).and_then(json_to_u128) {
                seen.insert(*key);
                let attr = build_nvme_attr(id_counter, key, display, val);
                apply_nvme_top_fields(key, val, disk);
                disk.nvme_attributes.push(attr);
                id_counter = id_counter.saturating_add(1);
            }
        }

        // Extra numeric fields in smartctl JSON (excluding nsid and array temperature_sensors)
        for (k, val_json) in log {
            if seen.contains(k.as_str()) || k == "nsid" || k == "temperature_sensors" {
                continue;
            }
            if let Some(val) = json_to_u128(val_json) {
                let display = to_title_case(&k.replace('_', " "));
                disk.nvme_attributes
                    .push(build_nvme_attr(id_counter, k, &display, val));
                id_counter = id_counter.saturating_add(1);
            }
        }

        if let Some(sensors) = log.get("temperature_sensors").and_then(|s| s.as_array()) {
            for (i, temp_val) in sensors.iter().enumerate() {
                if let Some(c) = json_to_i32(temp_val)
                    && c > 0
                    && c < 300
                {
                    let key = format!("temperature_sensor_{}", i + 1);
                    let name = format!("Temperature Sensor {}", i + 1);
                    disk.nvme_attributes.push(NvmeSmartAttribute {
                        id: id_counter,
                        key,
                        name: name.clone(),
                        raw_value: (c as i64 + 273) as u128,
                        decoded_value: format!("{} °C", c),
                        status: HealthStatus::Good,
                        note: format!("{} °C ({} K)", c, c + 273),
                    });
                    disk.temperature.sensors.push(TempSensor {
                        label: name,
                        celsius: c as f64,
                        max_celsius: Some(disk.temperature.warning_threshold as f64),
                        crit_celsius: Some(disk.temperature.critical_threshold as f64),
                    });
                    id_counter = id_counter.saturating_add(1);
                }
            }
        }
    }

    if let Some(st_log) = v.get("nvme_self_test_log") {
        if let Some(op) = st_log.get("current_self_test_operation") {
            let op_val = op.get("value").and_then(json_to_u64).unwrap_or(0);
            let status_text = op
                .get("string")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let in_progress =
                op_val != 0 || status_text.to_ascii_lowercase().contains("in progress");
            let completed_percent = if in_progress {
                st_log
                    .get("current_self_test_completion_percent")
                    .and_then(json_to_u64)
                    .map(|p| p.min(100) as u8)
            } else {
                None
            };
            let remaining_percent = completed_percent.map(|p| 100u8.saturating_sub(p));
            if !status_text.is_empty() || in_progress {
                disk.self_test_execution = Some(SelfTestExecutionStatus {
                    in_progress,
                    status_text,
                    remaining_percent,
                    completed_percent,
                });
            }
        }
        if let Some(tbl) = st_log.get("table").and_then(|t| t.as_array()) {
            for (idx, entry) in tbl.iter().enumerate() {
                disk.self_test_log.push(SelfTestEntry {
                    num: Some((idx + 1) as u32),
                    test_type: entry
                        .get("self_test_code")
                        .and_then(|c| c.get("string"))
                        .and_then(|s| s.as_str())
                        .unwrap_or("Unknown")
                        .to_string(),
                    status: entry
                        .get("self_test_result")
                        .and_then(|r| r.get("string"))
                        .and_then(|s| s.as_str())
                        .unwrap_or("Unknown")
                        .to_string(),
                    lifetime_hours: entry.get("power_on_hours").and_then(json_to_u64),
                    lba: entry
                        .get("failing_lba")
                        .and_then(json_to_u64)
                        .map(|l| format!("0x{:X}", l)),
                });
            }
        }
    }

    if let Some(tbl) = v
        .get("nvme_error_information_log")
        .and_then(|l| l.get("table"))
        .and_then(|t| t.as_array())
    {
        disk.error_log_count = v
            .get("nvme_error_information_log")
            .and_then(|l| l.get("size"))
            .and_then(json_to_u64)
            .or(Some(tbl.len() as u64));
        for entry in tbl {
            disk.error_logs.push(ErrorLogEntry {
                error_number: entry.get("error_count").and_then(json_to_u64),
                lifetime_hours: None,
                state_or_status: entry
                    .get("status_field")
                    .and_then(|s| s.get("string"))
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_string(),
                description: format!(
                    "SQID={} CID={} ParamLoc=0x{:X} LBA={}",
                    entry
                        .get("submission_queue_id")
                        .and_then(json_to_u64)
                        .unwrap_or(0),
                    entry.get("command_id").and_then(json_to_u64).unwrap_or(0),
                    entry
                        .get("parameter_error_location")
                        .and_then(json_to_u64)
                        .unwrap_or(0),
                    entry.get("lba").and_then(json_to_u64).unwrap_or(0)
                ),
            });
        }
    }
}

fn parse_scsi_sections(v: &Value, disk: &mut DiskInfo, eval_opts: &HealthEvalOptions) {
    disk.temperature.warning_threshold = eval_opts.temp_warning_ata;
    if disk.temperature.drive_critical_threshold.is_none() {
        disk.temperature.critical_threshold = eval_opts.temp_critical_ata;
    }
    disk.interface_label = v
        .get("scsi_transport_protocol")
        .and_then(|t| t.get("name"))
        .and_then(|s| s.as_str())
        .unwrap_or("SCSI / SAS")
        .to_string();
    disk.transfer_mode = "----".to_string();
    disk.standard = v
        .get("scsi_version")
        .and_then(|s| s.as_str())
        .unwrap_or("SCSI")
        .to_string();

    if disk.smart_supported == Some(true) || disk.smart_enabled == Some(true) {
        disk.features.push("S.M.A.R.T.".to_string());
    }

    if let Some(cycles) = v
        .get("scsi_start_stop_cycle_counter")
        .and_then(|c| c.get("accumulated_load_unload_cycles"))
        .and_then(json_to_u64)
    {
        disk.power_cycle_count = Some(cycles);
    }

    disk.scsi_grown_defect_list = v.get("scsi_grown_defect_list").and_then(json_to_u64);

    if let Some(err_log) = v.get("scsi_error_counter_log").and_then(|e| e.as_object()) {
        for op in ["read", "write", "verify"] {
            if let Some(op_obj) = err_log.get(op) {
                let total_uncorrected = op_obj
                    .get("total_uncorrected_errors")
                    .and_then(json_to_u64)
                    .unwrap_or(0);
                let gb_str = op_obj
                    .get("gigabytes_processed")
                    .and_then(|s| s.as_str())
                    .unwrap_or("0")
                    .to_string();

                if let Some(bytes) = parse_decimal_gb_to_bytes(&gb_str) {
                    if op == "read" {
                        disk.total_host_reads_bytes = Some(bytes);
                    } else if op == "write" {
                        disk.total_host_writes_bytes = Some(bytes);
                    }
                }

                disk.scsi_error_log.push(ScsiErrorCounterRow {
                    operation: op.to_string(),
                    ecc_fast: op_obj
                        .get("errors_corrected_by_eccfast")
                        .and_then(json_to_u64)
                        .unwrap_or(0),
                    ecc_delayed: op_obj
                        .get("errors_corrected_by_eccdelayed")
                        .and_then(json_to_u64)
                        .unwrap_or(0),
                    rereads_rewrites: op_obj
                        .get("errors_corrected_by_rereads_rewrites")
                        .and_then(json_to_u64)
                        .unwrap_or(0),
                    total_corrected: op_obj
                        .get("total_errors_corrected")
                        .and_then(json_to_u64)
                        .unwrap_or(0),
                    algorithm_invocations: op_obj
                        .get("correction_algorithm_invocations")
                        .and_then(json_to_u64)
                        .unwrap_or(0),
                    gigabytes_processed: gb_str,
                    total_uncorrected,
                    status: if total_uncorrected == 0 {
                        HealthStatus::Good
                    } else {
                        HealthStatus::Caution
                    },
                });
            }
        }
    }
}

pub fn evaluate_health(disk: &mut DiskInfo, opts: &HealthEvalOptions) {
    disk.health_reasons.clear();
    let mut caution = false;
    let mut bad = false;

    if disk.protocol == DiskProtocol::Ata {
        disk.temperature.warning_threshold = disk
            .temperature
            .drive_warning_threshold
            .unwrap_or(opts.temp_warning_ata);
        disk.temperature.critical_threshold = disk
            .temperature
            .drive_critical_threshold
            .unwrap_or(opts.temp_critical_ata);
        for attr in &mut disk.ata_attributes {
            let thresh_fail = attr.thresh > 0 && attr.value < attr.thresh;
            let when_failed = !attr.when_failed.is_empty() && attr.when_failed != "-";
            let failed_now = attr.when_failed.eq_ignore_ascii_case("now");
            let is_caution_attr = (attr.id == 5
                || attr.id == 197
                || attr.id == 198
                || (attr.id == 196 && !opts.ignore_c4))
                && attr.raw_value > 0;

            if thresh_fail || failed_now {
                attr.status = HealthStatus::Bad;
                bad = true;
                if thresh_fail {
                    disk.health_reasons.push(format!(
                        "ATA {:02X} {} ({} < {})",
                        attr.id, attr.name, attr.value, attr.thresh
                    ));
                } else {
                    disk.health_reasons.push(format!(
                        "ATA {:02X} {} (failed: {})",
                        attr.id, attr.name, attr.when_failed
                    ));
                }
            } else if when_failed {
                attr.status = HealthStatus::Caution;
                caution = true;
                disk.health_reasons.push(format!(
                    "ATA {:02X} {} (failed: {})",
                    attr.id, attr.name, attr.when_failed
                ));
            } else if is_caution_attr {
                attr.status = HealthStatus::Caution;
                caution = true;
                disk.health_reasons.push(format!(
                    "ATA {:02X} {} = {}",
                    attr.id, attr.name, attr.raw_value
                ));
            } else {
                attr.status = HealthStatus::Good;
            }
        }
    } else if disk.protocol == DiskProtocol::Nvme {
        disk.temperature.warning_threshold = disk
            .temperature
            .drive_warning_threshold
            .unwrap_or(opts.temp_warning_nvme);
        disk.temperature.critical_threshold = disk
            .temperature
            .drive_critical_threshold
            .unwrap_or(opts.temp_critical_nvme);

        let avail_spare = disk
            .nvme_attributes
            .iter()
            .find(|a| a.key == "available_spare")
            .map(|a| a.raw_value);
        let avail_spare_thresh = disk
            .nvme_attributes
            .iter()
            .find(|a| a.key == "available_spare_threshold")
            .map(|a| a.raw_value);

        for attr in &mut disk.nvme_attributes {
            attr.status = HealthStatus::Good;
            if attr.key == "critical_warning" && attr.raw_value != 0 {
                attr.status = HealthStatus::Bad;
                bad = true;
                let flags = decode_nvme_critical_warning(attr.raw_value as u32);
                attr.note = flags.join("; ");
                disk.health_reasons.extend(flags);
            } else if attr.key == "available_spare" {
                if let (Some(spare), Some(thresh)) = (avail_spare, avail_spare_thresh)
                    && thresh <= 100
                    && !(spare == 0 && thresh == 0)
                {
                    if spare < thresh {
                        attr.status = HealthStatus::Bad;
                        bad = true;
                        disk.health_reasons.push(format!(
                            "Available Spare ({}%) < Threshold ({}%)",
                            spare, thresh
                        ));
                    } else if thresh != 100 && spare == thresh {
                        attr.status = HealthStatus::Caution;
                        caution = true;
                        disk.health_reasons.push(format!(
                            "Available Spare ({}%) == Threshold ({}%)",
                            spare, thresh
                        ));
                    }
                }
            } else if attr.key == "percentage_used" {
                let remaining = 100u128.saturating_sub(attr.raw_value);
                if remaining <= opts.ssd_wear_warning_threshold_percent as u128 {
                    attr.status = HealthStatus::Caution;
                    caution = true;
                    disk.health_reasons.push(format!(
                        "SSD Remaining Life {}% (<= {}%)",
                        remaining, opts.ssd_wear_warning_threshold_percent
                    ));
                }
            }
        }
    } else if disk.protocol == DiskProtocol::Scsi {
        disk.temperature.warning_threshold = disk
            .temperature
            .drive_warning_threshold
            .unwrap_or(opts.temp_warning_ata);
        disk.temperature.critical_threshold = disk
            .temperature
            .drive_critical_threshold
            .unwrap_or(opts.temp_critical_ata);
        for row in &mut disk.scsi_error_log {
            if row.total_uncorrected > 0 {
                row.status = HealthStatus::Caution;
                caution = true;
                disk.health_reasons.push(format!(
                    "SCSI {} uncorrected errors: {}",
                    row.operation, row.total_uncorrected
                ));
            } else {
                row.status = HealthStatus::Good;
            }
        }
        if let Some(gdl) = disk.scsi_grown_defect_list
            && gdl > 0
        {
            caution = true;
            disk.health_reasons
                .push(format!("Grown Defect List: {}", gdl));
        }
    } else {
        disk.temperature.warning_threshold = disk
            .temperature
            .drive_warning_threshold
            .unwrap_or(opts.temp_warning_ata);
        disk.temperature.critical_threshold = disk
            .temperature
            .drive_critical_threshold
            .unwrap_or(opts.temp_critical_ata);
    }

    if disk.smart_passed == Some(false) {
        bad = true;
        if disk.health_reasons.is_empty() {
            disk.health_reasons
                .push("S.M.A.R.T. overall-health assessment FAILED".to_string());
        }
    }

    disk.health_status = match (disk.smart_passed, bad, caution) {
        (_, true, _) if !disk.model_name.is_empty() || disk.smart_passed.is_some() => {
            HealthStatus::Bad
        }
        (Some(true), false, true) => HealthStatus::Caution,
        (Some(true), false, false) => HealthStatus::Good,
        (Some(false), _, _) => HealthStatus::Bad,
        (None, false, true) if !disk.model_name.is_empty() => HealthStatus::Caution,
        _ => HealthStatus::Unknown,
    };
}

/// Merge sysfs + lsblk info into a `DiskInfo`, or create an unprivileged-only `DiskInfo`
/// when smartctl failed (permission denied or missing).
pub fn merge_sysfs_and_partitions(
    disk: &mut DiskInfo,
    sysfs: SysfsInfo,
    partitions: Vec<PartitionInfo>,
    hwmon_sensors: Vec<TempSensor>,
) {
    disk.partitions = partitions;
    disk.sysfs = sysfs;

    if disk.model_name.is_empty()
        && let Some(m) = &disk.sysfs.lsblk_model
    {
        disk.model_name = m.clone();
    }
    if disk.serial_number.is_empty()
        && let Some(s) = &disk.sysfs.lsblk_serial
    {
        disk.serial_number = s.clone();
    }
    if (disk.firmware_version.is_empty() || disk.firmware_version == "----")
        && let Some(f) = &disk.sysfs.lsblk_firmware
        && !f.is_empty()
    {
        disk.firmware_version = f.clone();
    }
    if disk.capacity_bytes == 0
        && let Some(sz) = disk.sysfs.lsblk_size_bytes
    {
        disk.capacity_bytes = sz;
    }
    if disk.logical_block_size == 0 {
        disk.logical_block_size = disk
            .sysfs
            .logical_block_size
            .filter(|&v| v > 0)
            .unwrap_or(512);
    }
    if disk.physical_block_size == 0 {
        disk.physical_block_size = disk
            .sysfs
            .physical_block_size
            .filter(|&v| v > 0)
            .unwrap_or(disk.logical_block_size);
    }
    if disk.rotation_rate.is_none()
        && let Some(rota) = disk.sysfs.rotational
        && !rota
    {
        disk.rotation_rate = Some(0);
    }
    if disk.protocol == DiskProtocol::Unknown {
        disk.protocol = match disk.sysfs.lsblk_tran.as_deref() {
            Some("nvme") => DiskProtocol::Nvme,
            Some("sata") | Some("ata") => DiskProtocol::Ata,
            Some("scsi") | Some("sas") => DiskProtocol::Scsi,
            _ => {
                if disk.device_node.contains("nvme") {
                    DiskProtocol::Nvme
                } else {
                    DiskProtocol::Unknown
                }
            }
        };
    }
    if disk.protocol_str.is_empty() || disk.protocol_str == "Unknown" {
        disk.protocol_str = match disk.sysfs.lsblk_tran.as_deref() {
            Some("nvme") => "NVMe".to_string(),
            Some("sata") | Some("ata") => "ATA".to_string(),
            Some("scsi") | Some("sas") => "SCSI".to_string(),
            Some("usb") => "USB".to_string(),
            Some(other) => other.to_uppercase(),
            None => "Unknown".to_string(),
        };
    }
    if disk.standard.is_empty() {
        disk.standard = match disk.protocol {
            DiskProtocol::Nvme => "NVM Express".to_string(),
            DiskProtocol::Ata => "Serial ATA".to_string(),
            DiskProtocol::Scsi => "SCSI".to_string(),
            DiskProtocol::Unknown => String::new(),
        };
    }
    if disk.device_type.is_empty()
        && let Some(tran) = &disk.sysfs.lsblk_tran
    {
        disk.device_type = tran.clone();
    }
    if disk.wwn.is_none() {
        disk.wwn = disk.sysfs.wwid.clone();
    }

    if let Some(block_name) = &disk.sysfs.block_name {
        disk.block_device_node = Some(format!("/dev/{}", block_name));
    }

    // Check SSD Wear Leveling Count if rotation_rate came from sysfs instead of smartctl
    if disk.health_percentage.is_none()
        && (disk.rotation_rate == Some(0) || disk.sysfs.rotational == Some(false))
        && let Some(attr) = disk
            .ata_attributes
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case("Wear Leveling Count"))
    {
        disk.health_percentage = Some(attr.value.min(100));
    }

    // Enrich Transfer Mode from PCIe link speed/width on NVMe if smartctl didn't report it
    if disk.protocol == DiskProtocol::Nvme || disk.sysfs.lsblk_tran.as_deref() == Some("nvme") {
        let cur = format_pcie_link(
            disk.sysfs.current_link_speed.as_deref(),
            disk.sysfs.current_link_width,
        );
        let max = format_pcie_link(
            disk.sysfs.max_link_speed.as_deref(),
            disk.sysfs.max_link_width,
        );
        if cur != "----" || max != "----" {
            disk.transfer_mode = format!("{} | {}", cur, max);
        } else if disk.transfer_mode.is_empty() {
            disk.transfer_mode = "----".to_string();
        }
        if disk.interface_label.is_empty() {
            disk.interface_label = "NVM Express".to_string();
        }
    } else if disk.interface_label.is_empty() {
        disk.interface_label = match disk.sysfs.lsblk_tran.as_deref() {
            Some("sata") => "Serial ATA".to_string(),
            Some("usb") => "USB".to_string(),
            Some("sas") | Some("scsi") => "SAS / SCSI".to_string(),
            Some(other) => other.to_uppercase(),
            None => "Block Device".to_string(),
        };
    }

    if disk.transfer_mode.is_empty() {
        if let Some(sata_speed) = &disk.sysfs.sata_link_speed {
            disk.transfer_mode = format_sata_speed(sata_speed);
        } else {
            disk.transfer_mode = "----".to_string();
        }
    }

    // Add TRIM / WriteCache / NCQ from sysfs if not already listed
    if disk.sysfs.discard_max_bytes.unwrap_or(0) > 0 && !disk.features.iter().any(|f| f == "TRIM") {
        disk.features.push("TRIM".to_string());
    }
    if disk.sysfs.write_cache.as_deref() == Some("write back")
        && !disk.features.iter().any(|f| f.contains("WriteCache"))
    {
        disk.features.push("VolatileWriteCache".to_string());
    }
    if disk.sysfs.ncq_prio_supported == Some(true) && !disk.features.iter().any(|f| f == "NCQ") {
        disk.features.push("NCQ".to_string());
    }

    // Merge hwmon temperature sensors (useful especially when unprivileged!)
    let normalize_sensor = |lbl: &str| -> String {
        let lower = lbl.trim().to_ascii_lowercase();
        lower
            .strip_prefix("temperature ")
            .unwrap_or(&lower)
            .to_string()
    };
    for s in hwmon_sensors {
        let norm_s = normalize_sensor(&s.label);
        if !disk
            .temperature
            .sensors
            .iter()
            .any(|existing| normalize_sensor(&existing.label) == norm_s)
        {
            if s.label.eq_ignore_ascii_case("Composite") || disk.temperature.sensors.is_empty() {
                if disk.temperature.current_celsius.is_none() {
                    let c = s.celsius.round() as i32;
                    if c > 0 && c < 300 {
                        disk.temperature.current_celsius = Some(c);
                    }
                }
                if disk.temperature.drive_warning_threshold.is_none()
                    && let Some(max_c) = s.max_celsius
                {
                    let val = max_c.round() as i32;
                    if val > 0 && val < 250 {
                        disk.temperature.drive_warning_threshold = Some(val);
                        disk.temperature.warning_threshold = val;
                    }
                }
                if disk.temperature.drive_critical_threshold.is_none()
                    && let Some(crit_c) = s.crit_celsius
                {
                    let val = crit_c.round() as i32;
                    if val > 0 && val < 250 {
                        disk.temperature.drive_critical_threshold = Some(val);
                        disk.temperature.critical_threshold = val;
                    }
                }
            }
            disk.temperature.sensors.push(s);
        }
    }
}

pub fn decode_nvme_critical_warning(cw: u32) -> Vec<String> {
    let mut out = Vec::new();
    if cw & 0x01 != 0 {
        out.push("Available spare capacity has fallen below the threshold".to_string());
    }
    if cw & 0x02 != 0 {
        out.push("Temperature error (Overheat or Overcool)".to_string());
    }
    if cw & 0x04 != 0 {
        out.push("NVM subsystem reliability has been degraded".to_string());
    }
    if cw & 0x08 != 0 {
        out.push("Media has been placed in Read Only Mode".to_string());
    }
    if cw & 0x10 != 0 {
        out.push("Volatile memory backup device has failed".to_string());
    }
    if cw & 0x20 != 0 {
        out.push("Persistent memory region has become Read-Only".to_string());
    }
    if out.is_empty() && cw != 0 {
        out.push(format!("Critical Warning flags: 0x{:X}", cw));
    }
    out
}

fn build_nvme_attr(id: u8, key: &str, name: &str, val: u128) -> NvmeSmartAttribute {
    let (decoded_value, note) = match key {
        "critical_warning" => {
            if val == 0 {
                ("0 (OK)".to_string(), "No critical warnings".to_string())
            } else {
                let flags = decode_nvme_critical_warning(val as u32);
                (format!("0x{:02X} (!)", val), flags.join("; "))
            }
        }
        "temperature" => {
            let c = (val as i64 - 273) as i32;
            (
                crate::model::format_temperature(Some(c), false),
                format!("{} °C ({} K)", c, val),
            )
        }
        "available_spare" | "available_spare_threshold" | "percentage_used" => {
            (format!("{} %", val), format!("{} %", val))
        }
        "data_units_read" | "data_units_written" => {
            let bytes = val * 512_000;
            let pretty = crate::model::format_data_volume(Some(bytes), false);
            (pretty.clone(), format!("{} x 512,000 B = {}", val, pretty))
        }
        "controller_busy_time" | "warning_temp_time" | "critical_comp_time" => (
            format!("{} min", val),
            format!("{} min ({:.1} h)", val, val as f64 / 60.0),
        ),
        "power_on_hours" => (
            format!("{} h", val),
            format!("{} hours ({:.1} days)", val, val as f64 / 24.0),
        ),
        _ => (val.to_string(), String::new()),
    };

    NvmeSmartAttribute {
        id,
        key: key.to_string(),
        name: name.to_string(),
        raw_value: val,
        decoded_value,
        status: HealthStatus::Good,
        note,
    }
}

fn apply_nvme_top_fields(key: &str, val: u128, disk: &mut DiskInfo) {
    match key {
        "data_units_read" => {
            disk.total_host_reads_bytes = Some(val * 512_000);
        }
        "data_units_written" => {
            disk.total_host_writes_bytes = Some(val * 512_000);
        }
        "percentage_used" => {
            let rem = 100u128.saturating_sub(val).min(100) as u8;
            disk.health_percentage = Some(rem);
        }
        "power_cycles" => {
            if disk.power_cycle_count.is_none() {
                disk.power_cycle_count = Some(val as u64);
            }
        }
        "power_on_hours" => {
            if disk.power_on_hours.is_none() {
                disk.power_on_hours = Some(val as u64);
            }
        }
        "unsafe_shutdowns" => {
            disk.unsafe_shutdowns = Some(val as u64);
        }
        "temperature" if disk.temperature.current_celsius.is_none() => {
            let c = (val as i64 - 273) as i32;
            if c > 0 && c < 300 {
                disk.temperature.current_celsius = Some(c);
            }
        }
        _ => {}
    }
}

fn decode_ata_attribute_note(
    id: u8,
    raw_name: &str,
    raw_value: u64,
    raw_string: &str,
    prefailure: bool,
) -> String {
    let mut parts = Vec::new();
    if prefailure {
        parts.push("Pre-fail".to_string());
    }
    match id {
        9 => {
            if !raw_string.is_empty() && raw_value > 1_000_000 {
                parts.push(raw_string.to_string());
            } else {
                let hrs = raw_value;
                parts.push(format!("{} h ({:.1} d)", hrs, hrs as f64 / 24.0));
            }
        }
        190 | 194 => {
            if !raw_string.is_empty() {
                if raw_string.chars().all(|c| c.is_ascii_digit() || c == '-') {
                    parts.push(format!("{} °C", raw_string));
                } else {
                    parts.push(format!("{} (°C)", raw_string));
                }
            }
        }
        _ if !raw_string.is_empty() && raw_string != raw_value.to_string() => {
            parts.push(raw_string.to_string());
        }
        _ if raw_name.contains("Hours") => {
            parts.push(format!("{} h", raw_value));
        }
        _ => {}
    }
    parts.join(" • ")
}

fn format_sata_speed(s: &str) -> String {
    if s.contains("6.0 Gb/s") || s.contains("6.0 Gbps") {
        "SATA/600 (6.0 Gb/s)".to_string()
    } else if s.contains("3.0 Gb/s") || s.contains("3.0 Gbps") {
        "SATA/300 (3.0 Gb/s)".to_string()
    } else if s.contains("1.5 Gb/s") || s.contains("1.5 Gbps") {
        "SATA/150 (1.5 Gb/s)".to_string()
    } else {
        s.to_string()
    }
}

pub fn format_pcie_link(speed: Option<&str>, width: Option<u32>) -> String {
    let Some(speed) = speed else {
        return "----".to_string();
    };
    let pcie_gen = if speed.starts_with("2.5") {
        "PCIe 1.0"
    } else if speed.starts_with("5.0") {
        "PCIe 2.0"
    } else if speed.starts_with("8.0") {
        "PCIe 3.0"
    } else if speed.starts_with("16.0") {
        "PCIe 4.0"
    } else if speed.starts_with("32.0") {
        "PCIe 5.0"
    } else if speed.starts_with("64.0") {
        "PCIe 6.0"
    } else {
        speed.trim()
    };
    match width {
        Some(w) if w > 0 => format!("{} x{}", pcie_gen, w),
        _ => pcie_gen.to_string(),
    }
}

fn extract_temp_from_threshold_line(line: &str) -> Option<i32> {
    let (_, after) = line.split_once(':')?;
    let first_tok = after.split_whitespace().next()?;
    first_tok.parse::<i32>().ok()
}

pub fn parse_nvme_power_state_line(line: &str) -> Option<NvmePowerStateInfo> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 4 {
        return None;
    }
    let state = tokens[0].parse::<u32>().ok()?;
    let mut is_active = false;
    let is_non_operational = match tokens[1] {
        "*" => {
            is_active = true;
            match *tokens.get(2)? {
                "-" => true,
                "+" => false,
                _ => return None,
            }
        }
        "*+" => {
            is_active = true;
            false
        }
        "*-" => {
            is_active = true;
            true
        }
        "+*" => {
            is_active = true;
            false
        }
        "-*" => {
            is_active = true;
            true
        }
        "+" => {
            if tokens.get(2).copied() == Some("*") {
                is_active = true;
            }
            false
        }
        "-" => {
            if tokens.get(2).copied() == Some("*") {
                is_active = true;
            }
            true
        }
        _ => return None,
    };

    // Find wattage token like `5.50W` or `0.0050W`
    let max_power_watts = tokens.iter().find_map(|tok| {
        tok.strip_suffix('W')
            .and_then(|num| num.parse::<f64>().ok())
    });
    let (entry_latency_us, exit_latency_us) = if tokens.len() >= 11 {
        (
            tokens[tokens.len() - 2].parse::<u64>().ok(),
            tokens[tokens.len() - 1].parse::<u64>().ok(),
        )
    } else {
        (None, None)
    };
    Some(NvmePowerStateInfo {
        state,
        max_power_watts,
        is_non_operational,
        is_active,
        entry_latency_us,
        exit_latency_us,
        description: line.to_string(),
    })
}

fn parse_decimal_gb_to_bytes(s: &str) -> Option<u128> {
    let normalized = s.trim().replace(',', ".");
    let gb: f64 = normalized.parse().ok()?;
    if gb < 0.0 {
        return None;
    }
    Some((gb * 1e9) as u128)
}

fn parse_ata_extra_features(v: &Value, disk: &mut DiskInfo) {
    if let Some(apm) = v.get("ata_apm") {
        let enabled = apm
            .get("enabled")
            .and_then(|b| b.as_bool())
            .unwrap_or(false);
        let level = apm.get("level").and_then(json_to_u64).map(|x| x as u32);
        disk.ata_apm = Some(AtaApmInfo { enabled, level });
    }
    if let Some(aam) = v.get("ata_aam") {
        let enabled = aam
            .get("enabled")
            .and_then(|b| b.as_bool())
            .unwrap_or(false);
        let level = aam.get("level").and_then(json_to_u64).map(|x| x as u32);
        let recommended_level = aam
            .get("recommended_level")
            .and_then(json_to_u64)
            .map(|x| x as u32);
        disk.ata_aam = Some(AtaAamInfo {
            enabled,
            level,
            recommended_level,
        });
    }
    if let Some(sec) = v.get("ata_security") {
        let raw_state = sec.get("state").and_then(json_to_u64);
        disk.ata_security = Some(AtaSecurityInfo {
            state_string: sec
                .get("string")
                .and_then(|s| s.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            enabled: sec
                .get("enabled")
                .and_then(|b| b.as_bool())
                .unwrap_or_else(|| raw_state.is_some_and(|st| (st & 0x02) != 0)),
            locked: sec
                .get("locked")
                .and_then(|b| b.as_bool())
                .unwrap_or_else(|| raw_state.is_some_and(|st| (st & 0x04) != 0)),
            frozen: sec
                .get("frozen")
                .and_then(|b| b.as_bool())
                .unwrap_or_else(|| raw_state.is_some_and(|st| (st & 0x08) != 0)),
            expired: sec
                .get("expired")
                .and_then(|b| b.as_bool())
                .unwrap_or_else(|| raw_state.is_some_and(|st| (st & 0x10) != 0)),
            normal_erase_minutes: sec
                .get("normal_time_minutes")
                .or_else(|| sec.get("erase_minutes"))
                .and_then(json_to_u64)
                .map(|x| x as u32),
            extended_erase_minutes: sec
                .get("extended_time_minutes")
                .or_else(|| sec.get("enhanced_erase_minutes"))
                .and_then(json_to_u64)
                .map(|x| x as u32),
        });
    }
    if v.get("ata_sct_capabilities").is_some()
        || v.get("ata_sct_status").is_some()
        || v.get("ata_sct_temperature_history").is_some()
    {
        let caps = v.get("ata_sct_capabilities");
        let status = v.get("ata_sct_status");
        let hist = v.get("ata_sct_temperature_history");
        let table = hist
            .and_then(|h| h.get("table"))
            .and_then(|t| t.as_array())
            .map(|arr| arr.iter().map(json_to_i32).collect())
            .unwrap_or_default();
        let device_state = status
            .and_then(|s| s.get("device_state").or_else(|| s.get("state")))
            .and_then(|d| {
                d.get("string")
                    .and_then(|s| s.as_str())
                    .or_else(|| d.as_str())
            })
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        disk.ata_sct = Some(AtaSctInfo {
            error_recovery_control_supported: caps
                .and_then(|c| c.get("error_recovery_control_supported"))
                .and_then(|b| b.as_bool())
                .unwrap_or(false),
            feature_control_supported: caps
                .and_then(|c| c.get("feature_control_supported"))
                .and_then(|b| b.as_bool())
                .unwrap_or(false),
            data_table_supported: caps
                .and_then(|c| c.get("data_table_supported"))
                .and_then(|b| b.as_bool())
                .unwrap_or(false),
            device_state,
            sampling_period_minutes: hist
                .and_then(|h| h.get("sampling_period_minutes"))
                .and_then(json_to_u64)
                .map(|x| x as u32),
            logging_interval_minutes: hist
                .and_then(|h| h.get("logging_interval_minutes"))
                .and_then(json_to_u64)
                .map(|x| x as u32),
            temperature_history: table,
        });
    }
}

fn extract_remaining_percent_from_text(text: &str) -> Option<u8> {
    let lower = text.to_ascii_lowercase();
    let idx = lower.find("% remaining")?;
    let prefix = lower[..idx].trim_end();
    let digits: String = prefix
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    let val = digits.parse::<u8>().ok()?;
    Some(val.min(100))
}

fn json_to_u64(v: &Value) -> Option<u64> {
    if let Some(u) = v.as_u64() {
        Some(u)
    } else if let Some(i) = v.as_i64() {
        u64::try_from(i).ok()
    } else {
        v.as_f64().filter(|f| *f >= 0.0).map(|f| f as u64)
    }
}

fn json_to_u128(v: &Value) -> Option<u128> {
    if let Some(u) = v.as_u64() {
        Some(u as u128)
    } else if let Some(i) = v.as_i64() {
        u128::try_from(i).ok()
    } else {
        v.as_f64().filter(|f| *f >= 0.0).map(|f| f as u128)
    }
}

fn json_to_i32(v: &Value) -> Option<i32> {
    if let Some(i) = v.as_i64() {
        i32::try_from(i).ok()
    } else if let Some(u) = v.as_u64() {
        i32::try_from(u).ok()
    } else {
        v.as_f64().map(|f| f.round() as i32)
    }
}

fn format_json_scalar(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        other => other.to_string(),
    }
}

pub fn to_title_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut cap_next = true;
    for ch in s.chars() {
        if ch.is_alphabetic() {
            if cap_next {
                for up in ch.to_uppercase() {
                    out.push(up);
                }
                cap_next = false;
            } else {
                for lo in ch.to_lowercase() {
                    out.push(lo);
                }
            }
        } else {
            out.push(ch);
            if ch == ' ' || ch == '-' || ch == '/' {
                cap_next = true;
            }
        }
    }
    out
}
