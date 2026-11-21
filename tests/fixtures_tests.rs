use rdiskinfo::config::{AppConfig, ThemeKind};
use rdiskinfo::export::{export_disk_json, format_full_report, format_single_disk_report};
use rdiskinfo::i18n::{Language, LanguageChoice, Strings, parse_locale_string};
use rdiskinfo::model::{
    DiskInfo, DiskProtocol, HealthEvalOptions, HealthStatus, PartitionInfo, SysfsInfo, TempSample,
    TemperatureInfo, format_capacity, format_data_volume, format_scsi_level, format_temperature,
    temp_status,
};
use rdiskinfo::privilege::{
    ElevationState, SelfTestOutcome, SelfTestResult, build_abort_self_test_args,
    build_start_self_test_args, collect_scan_targets, parse_self_test_response, validate_dev_type,
};
use rdiskinfo::raw_ioctl::{
    SgIoHdr, build_ata_passthrough_12, build_ata_passthrough_16, bytes_to_hex, hex_dump,
    hex_to_bytes, sg_io_succeeded, validate_device_path,
};
use rdiskinfo::smartctl::{
    ScannedDevice, evaluate_health, format_pcie_link, merge_sysfs_and_partitions,
    parse_nvme_power_state_line, parse_scan_json, parse_smartctl_device_json, split_json_documents,
};
use rdiskinfo::sysfs::{
    BlockDeviceBundle, find_ata_link_speed_with_class_dir, find_bundle_for_device,
    find_bundle_for_smart_device, is_raid_controller_dev_type, nvme_controller_name,
    parse_block_stat, read_hwmon_sensors,
};
use rdiskinfo::theme::ThemePalette;
use rdiskinfo::ui::{
    artwork, details_tabs,
    dialogs::{self, DialogStates, DialogViewContext, SelfTestModalState},
    disk_bar, find_selected_disk_index, record_temperature_samples, smart_table, summary_panel,
};

const ATA_SSD_JSON: &str = r#"{
  "json_format_version": [1, 0],
  "smartctl": {
    "version": [7, 5],
    "output": [
      "smartctl 7.5",
      "SATA Version is: SATA 3.3, 6.0 Gb/s (current: 6.0 Gb/s)"
    ],
    "exit_status": 0
  },
  "device": { "name": "/dev/sda", "type": "sat", "protocol": "ATA" },
  "model_family": "Crucial/Micron Client SSDs",
  "model_name": "CT1000BX500SSD1",
  "serial_number": "2244E68A1234",
  "firmware_version": "M6CR061",
  "wwn": { "naa": 5, "oui": 103637, "id": 123456789 },
  "user_capacity": { "blocks": 1953525168, "bytes": 1000204886016 },
  "logical_block_size": 512,
  "physical_block_size": 4096,
  "rotation_rate": 0,
  "form_factor": { "ata_value": 3, "name": "2.5 inches" },
  "trim": { "supported": true },
  "write_cache": { "supported": true, "enabled": true },
  "read_lookahead": { "supported": true, "enabled": true },
  "ata_version": { "string": "ACS-3 T13/2161-D revision 5" },
  "sata_version": { "string": "SATA 3.3" },
  "interface_speed": {
    "max": { "string": "6.0 Gb/s" },
    "current": { "string": "6.0 Gb/s" }
  },
  "smart_support": { "available": true, "enabled": true },
  "smart_status": { "passed": true },
  "ata_smart_attributes": {
    "table": [
      {
        "id": 1, "name": "Raw_Read_Error_Rate", "value": 100, "worst": 100, "thresh": 0,
        "when_failed": "", "flags": { "string": "POSR--", "prefailure": true, "updated_online": true },
        "raw": { "value": 0, "string": "0" }
      },
      {
        "id": 5, "name": "Reallocate_NAND_Blk_Cnt", "value": 100, "worst": 100, "thresh": 10,
        "when_failed": "", "flags": { "string": "PO--CK", "prefailure": true, "updated_online": true },
        "raw": { "value": 0, "string": "0" }
      },
      {
        "id": 9, "name": "Power_On_Hours", "value": 100, "worst": 100, "thresh": 0,
        "when_failed": "", "flags": { "string": "-O--CK", "prefailure": false, "updated_online": true },
        "raw": { "value": 4537, "string": "4537" }
      },
      {
        "id": 196, "name": "Reallocated_Event_Count", "value": 100, "worst": 100, "thresh": 0,
        "when_failed": "", "flags": { "string": "-O--CK", "prefailure": false, "updated_online": true },
        "raw": { "value": 3, "string": "3" }
      },
      {
        "id": 202, "name": "Percent_Lifetime_Remain", "value": 98, "worst": 98, "thresh": 0,
        "when_failed": "", "flags": { "string": "-O--CK", "prefailure": false, "updated_online": true },
        "raw": { "value": 2, "string": "2" }
      },
      {
        "id": 246, "name": "Total_LBAs_Written", "value": 100, "worst": 100, "thresh": 0,
        "when_failed": "", "flags": { "string": "-O--CK", "prefailure": false, "updated_online": true },
        "raw": { "value": 16777216000, "string": "16777216000" }
      }
    ]
  },
  "ata_smart_data": {
    "self_test": { "polling_minutes": { "short": 2, "extended": 45 } }
  },
  "ata_smart_self_test_log": {
    "standard": {
      "table": [
        { "num": 1, "type": { "string": "Short offline" }, "status": { "string": "Completed without error" }, "lifetime_hours": 4500 }
      ]
    }
  },
  "power_on_time": { "hours": 4537 },
  "power_cycle_count": 973,
  "temperature": { "current": 28, "lifetime_min": 18, "lifetime_max": 52 }
}"#;

const NVME_SSD_JSON: &str = r#"{
  "json_format_version": [1, 0],
  "smartctl": {
    "version": [7, 5],
    "output": [
      "Optional Admin Commands: 0x0017 Format_NVM Self_Test",
      "Optional NVM Commands: 0x005f Dataset_Mgmt Write_Zeroes",
      "Warning  Comp. Temp. Threshold:     82 Celsius",
      "Critical Comp. Temp. Threshold:     85 Celsius",
      "Supported Power States",
      "St Op     Max   Active     Idle   RL RT WL WT  Ent_Lat  Ex_Lat",
      " 0 +     6.50W       -        -    0  0  0  0        0       0",
      " 1 +     5.80W       -        -    1  1  1  1        0       0"
    ],
    "exit_status": 0
  },
  "device": { "name": "/dev/nvme0", "type": "nvme", "protocol": "NVMe" },
  "model_name": "Samsung SSD 990 PRO 2TB",
  "serial_number": "S73WNJ0W123456",
  "firmware_version": "1B2QJXD7",
  "nvme_version": { "string": "2.0" },
  "nvme_pci_vendor": { "id": 5197, "subsystem_id": 5197 },
  "nvme_ieee_oui_identifier": 9528,
  "nvme_controller_id": 6,
  "nvme_total_capacity": 2000398934016,
  "nvme_unallocated_capacity": 0,
  "nvme_namespaces": [
    { "id": 1, "size": { "bytes": 2000398934016 }, "capacity": { "bytes": 2000398934016 }, "utilization": { "bytes": 1000000000000 }, "formatted_lba_size": 4096 }
  ],
  "user_capacity": { "blocks": 488378646, "bytes": 2000398934016 },
  "logical_block_size": 4096,
  "smart_status": { "passed": true },
  "nvme_smart_health_information_log": {
    "nsid": 4294967295,
    "critical_warning": 0,
    "temperature": 52,
    "available_spare": 100,
    "available_spare_threshold": 10,
    "percentage_used": 0,
    "data_units_read": 3428021,
    "data_units_written": 2610145,
    "host_reads": 6970000000,
    "host_writes": 3890000000,
    "controller_busy_time": 120,
    "power_cycles": 539,
    "power_on_hours": 50,
    "unsafe_shutdowns": 4,
    "media_errors": 0,
    "num_err_log_entries": 0,
    "temperature_sensors": [52, 48]
  },
  "temperature": { "current": 52 },
  "power_cycle_count": 539,
  "power_on_time": { "hours": 50 }
}"#;

const SCSI_DRIVE_JSON: &str = r#"{
  "json_format_version": [1, 0],
  "smartctl": { "version": [7, 5], "output": [], "exit_status": 0 },
  "device": { "name": "/dev/sdb", "type": "scsi", "protocol": "SCSI" },
  "scsi_model_name": "SEAGATE ST4000NM0023",
  "scsi_revision": "0004",
  "serial_number": "Z1Z09876",
  "scsi_version": "SPC-4",
  "scsi_transport_protocol": { "name": "SAS (SPL-3)" },
  "user_capacity": { "bytes": 4000787030016 },
  "logical_block_size": 512,
  "rotation_rate": 7200,
  "smart_status": { "passed": true },
  "temperature": { "current": 37, "drive_trip": 68 },
  "power_on_time": { "hours": 12000 },
  "scsi_start_stop_cycle_counter": { "accumulated_load_unload_cycles": 45 },
  "scsi_grown_defect_list": 2,
  "scsi_error_counter_log": {
    "read": {
      "errors_corrected_by_eccfast": 12,
      "errors_corrected_by_eccdelayed": 0,
      "errors_corrected_by_rereads_rewrites": 0,
      "total_errors_corrected": 12,
      "correction_algorithm_invocations": 12,
      "gigabytes_processed": "1250,5",
      "total_uncorrected_errors": 0
    },
    "write": {
      "errors_corrected_by_eccfast": 0,
      "errors_corrected_by_eccdelayed": 0,
      "errors_corrected_by_rereads_rewrites": 0,
      "total_errors_corrected": 0,
      "correction_algorithm_invocations": 0,
      "gigabytes_processed": "800.2",
      "total_uncorrected_errors": 1
    },
    "verify": {
      "errors_corrected_by_eccfast": 0,
      "errors_corrected_by_eccdelayed": 0,
      "errors_corrected_by_rereads_rewrites": 0,
      "total_errors_corrected": 0,
      "correction_algorithm_invocations": 0,
      "gigabytes_processed": "0",
      "total_uncorrected_errors": 0
    }
  }
}"#;

#[test]
fn test_ata_ssd_parsing_and_ignore_c4_toggle() {
    let mut opts = HealthEvalOptions::default();
    let mut disk = parse_smartctl_device_json(ATA_SSD_JSON, None, None, &opts).unwrap();

    assert_eq!(disk.protocol, DiskProtocol::Ata);
    assert_eq!(disk.model_name, "CT1000BX500SSD1");
    assert_eq!(disk.firmware_version, "M6CR061");
    assert_eq!(disk.health_percentage, Some(98));
    assert_eq!(disk.health_status, HealthStatus::Good);
    assert_eq!(disk.rotation_rate, Some(0));
    assert_eq!(disk.power_on_hours, Some(4537));
    assert_eq!(disk.power_cycle_count, Some(973));
    assert_eq!(disk.temperature.current_celsius, Some(28));
    assert!(disk.features.contains(&"TRIM".to_string()));
    assert!(disk.features.contains(&"VolatileWriteCache".to_string()));
    assert!(disk.features.contains(&"S.M.A.R.T.".to_string()));
    assert_eq!(
        disk.self_test_polling_minutes,
        vec![("short".into(), 2), ("extended".into(), 45)]
    );
    assert_eq!(disk.self_test_log.len(), 1);
    // 16777216000 * 512 = 8589934592000 bytes (~8.59 TB)
    assert_eq!(disk.total_host_writes_bytes, Some(16_777_216_000u128 * 512));

    // Toggle ignore_c4 OFF -> C4=3 makes health Caution!
    opts.ignore_c4 = false;
    evaluate_health(&mut disk, &opts);
    assert_eq!(disk.health_status, HealthStatus::Caution);
    let c4 = disk.ata_attributes.iter().find(|a| a.id == 196).unwrap();
    assert_eq!(c4.status, HealthStatus::Caution);
}

#[test]
fn test_ata_brand_new_zero_hours_no_div_zero_and_64bit_raw() {
    let json = r#"{
      "device": { "name": "/dev/sda", "type": "sat", "protocol": "ATA" },
      "model_name": "TEST_ZERO_HOUR_SSD",
      "smart_status": { "passed": true },
      "logical_block_size": 512,
      "power_on_time": { "hours": 0 },
      "ata_smart_attributes": {
        "table": [
          { "id": 5, "name": "Reallocated_Sector_Ct", "value": 100, "worst": 100, "thresh": 10, "raw": { "value": 0, "string": "0" } },
          { "id": 1, "name": "Raw_Read_Error_Rate", "value": 100, "worst": 100, "thresh": 50, "raw": { "value": 281474976710655, "string": "281474976710655" } },
          { "id": 241, "name": "Total_LBAs_Written", "value": 100, "worst": 100, "thresh": 0, "raw": { "value": 1000, "string": "1000" } }
        ]
      }
    }"#;

    let disk = parse_smartctl_device_json(json, None, None, &HealthEvalOptions::default()).unwrap();
    assert_eq!(disk.health_status, HealthStatus::Good);
    assert_eq!(disk.total_host_writes_bytes, Some(512_000));
    let attr1 = disk.ata_attributes.iter().find(|a| a.id == 1).unwrap();
    assert_eq!(attr1.raw_value, 0xFFFF_FFFF_FFFF);
}

#[test]
fn test_ata_threshold_failure_is_bad() {
    let json = r#"{
      "device": { "name": "/dev/sda", "type": "sat", "protocol": "ATA" },
      "model_name": "FAILING_HDD",
      "smart_status": { "passed": true },
      "ata_smart_attributes": {
        "table": [
          { "id": 5, "name": "Reallocated_Sector_Ct", "value": 9, "worst": 9, "thresh": 10, "raw": { "value": 500, "string": "500" } }
        ]
      }
    }"#;
    let disk = parse_smartctl_device_json(json, None, None, &HealthEvalOptions::default()).unwrap();
    assert_eq!(disk.health_status, HealthStatus::Bad);
}

#[test]
fn test_nvme_64bit_counters_thresholds_and_bitmask_warnings() {
    let opts = HealthEvalOptions::default();
    let disk = parse_smartctl_device_json(NVME_SSD_JSON, None, None, &opts).unwrap();

    assert_eq!(disk.protocol, DiskProtocol::Nvme);
    assert_eq!(disk.standard, "NVM Express 2.0");
    assert_eq!(disk.health_status, HealthStatus::Good);
    assert_eq!(disk.health_percentage, Some(100));
    assert!(disk.nvme_self_test_supported);
    assert!(disk.features.contains(&"TRIM".to_string()));
    assert_eq!(disk.temperature.warning_threshold, 82);
    assert_eq!(disk.temperature.critical_threshold, 85);
    assert_eq!(disk.nvme_power_states.len(), 2);
    assert_eq!(disk.nvme_namespaces.len(), 1);
    assert_eq!(disk.temperature.sensors.len(), 2);
    // host_reads exceeded u32::MAX (6.97e9) -> no overflow
    let host_reads_attr = disk
        .nvme_attributes
        .iter()
        .find(|a| a.key == "host_reads")
        .unwrap();
    assert_eq!(host_reads_attr.raw_value, 6_970_000_000);
    // nsid must NOT appear in nvme_attributes
    assert!(!disk.nvme_attributes.iter().any(|a| a.key == "nsid"));
    // Data units read on 4Kn drive still uses 512_000 bytes/unit per NVMe spec!
    assert_eq!(disk.total_host_reads_bytes, Some(3_428_021u128 * 512_000));

    // Test bitmask critical_warning = 0x09 (bit 0 + bit 3)
    let bad_nvme_json = NVME_SSD_JSON.replace("\"critical_warning\": 0", "\"critical_warning\": 9");
    let bad_disk = parse_smartctl_device_json(&bad_nvme_json, None, None, &opts).unwrap();
    assert_eq!(bad_disk.health_status, HealthStatus::Bad);
    assert!(
        bad_disk
            .health_reasons
            .iter()
            .any(|r| r.contains("Available spare"))
    );
    assert!(
        bad_disk
            .health_reasons
            .iter()
            .any(|r| r.contains("Read Only"))
    );

    // Test percentage_used = 90 -> Caution (10% remaining <= 10% threshold)
    let worn_json = NVME_SSD_JSON.replace("\"percentage_used\": 0", "\"percentage_used\": 90");
    let worn_disk = parse_smartctl_device_json(&worn_json, None, None, &opts).unwrap();
    assert_eq!(worn_disk.health_percentage, Some(10));
    assert_eq!(worn_disk.health_status, HealthStatus::Caution);
}

#[test]
fn test_scsi_error_counters_and_grown_defects() {
    let disk =
        parse_smartctl_device_json(SCSI_DRIVE_JSON, None, None, &HealthEvalOptions::default())
            .unwrap();
    assert_eq!(disk.protocol, DiskProtocol::Scsi);
    assert_eq!(disk.model_name, "SEAGATE ST4000NM0023");
    assert_eq!(disk.power_cycle_count, Some(45));
    assert_eq!(disk.scsi_grown_defect_list, Some(2));
    assert_eq!(disk.health_status, HealthStatus::Caution);
    assert_eq!(disk.total_host_reads_bytes, Some(1_250_500_000_000));
    assert_eq!(disk.temperature.critical_threshold, 68);
}

#[test]
fn test_temperature_status_thresholds_fixed() {
    let mut t = TemperatureInfo {
        warning_threshold: 55,
        critical_threshold: 60,
        ..Default::default()
    };

    t.current_celsius = None;
    assert_eq!(temp_status(&t), HealthStatus::Unknown);
    t.current_celsius = Some(0);
    assert_eq!(temp_status(&t), HealthStatus::Unknown);
    t.current_celsius = Some(40);
    assert_eq!(temp_status(&t), HealthStatus::Good);
    t.current_celsius = Some(55);
    assert_eq!(temp_status(&t), HealthStatus::Caution);
    t.current_celsius = Some(59);
    assert_eq!(temp_status(&t), HealthStatus::Caution);
    t.current_celsius = Some(60);
    assert_eq!(temp_status(&t), HealthStatus::Bad);

    assert_eq!(format_temperature(None, false), "-- °C");
    assert_eq!(format_temperature(None, true), "-- °F");
    assert_eq!(format_temperature(Some(25), true), "77 °F");
}

#[test]
fn test_scan_and_split_json_documents() {
    let scan_out = r#"{ "devices": [
      { "name": "/dev/sda", "info_name": "/dev/sda", "type": "scsi", "protocol": "SCSI" },
      { "name": "/dev/nvme0", "info_name": "/dev/nvme0", "type": "nvme", "protocol": "NVMe" }
    ]}"#;
    let scanned = parse_scan_json(scan_out);
    assert_eq!(scanned.len(), 2);
    assert_eq!(scanned[1].name, "/dev/nvme0");

    let combined = format!("{}\n{}", ATA_SSD_JSON, NVME_SSD_JSON);
    let split = split_json_documents(&combined);
    assert_eq!(split.len(), 2);
}

#[test]
fn test_sysfs_helpers_and_formatting() {
    assert_eq!(
        format_pcie_link(Some("8.0 GT/s PCIe"), Some(4)),
        "PCIe 3.0 x4"
    );
    assert_eq!(
        format_pcie_link(Some("16.0 GT/s PCIe"), Some(4)),
        "PCIe 4.0 x4"
    );
    assert_eq!(format_capacity(1_000_204_886_016, false), "1 TB");
    assert_eq!(format_capacity(1_000_204_886_016, true), "1000.2 GB");
    assert_eq!(
        format_data_volume(Some(8_000_000_000_000), false),
        "8.00 TB"
    );

    let stat = parse_block_stat("100 5 2048 50 300 10 8192 120 2 90 170 4 0 512 8")
        .expect("valid block stat");
    assert_eq!(stat.reads_completed, 100);
    assert_eq!(stat.writes_completed, 300);
    assert_eq!(stat.io_in_flight, 2);
    assert_eq!(stat.discards_completed, 4);
    assert_eq!(stat.sectors_discarded, 512);
}

#[test]
fn test_device_path_validation_and_hex_dump() {
    assert!(validate_device_path("/dev/sda").is_ok());
    assert!(validate_device_path("/dev/nvme0").is_ok());
    assert!(validate_device_path("/etc/passwd").is_err());
    assert!(validate_device_path("/dev/../etc/passwd").is_err());
    assert!(validate_device_path("/dev/sda;id").is_err());

    let sample = b"RDiskInfo\x00\x01\x7fTEST";
    let dump = hex_dump(sample);
    assert!(dump.starts_with("00000000  52 44 69 73 6B 49 6E 66  6F 00 01 7F 54 45 53 54"));
    assert!(dump.contains("RDiskInfo...TEST"));

    let hex = bytes_to_hex(sample);
    let back = hex_to_bytes(&hex).unwrap();
    assert_eq!(back, sample);
}

#[test]
fn test_i18n_locale_detection_and_all_catalogs() {
    assert_eq!(parse_locale_string("pt_BR.UTF-8"), Some(Language::PtBr));
    assert_eq!(parse_locale_string("pt_PT.UTF-8"), Some(Language::PtBr));
    assert_eq!(parse_locale_string("en_US.UTF-8"), Some(Language::En));
    assert_eq!(parse_locale_string("es_ES.UTF-8:en"), Some(Language::Es));
    assert_eq!(parse_locale_string("fr_FR.UTF-8"), Some(Language::Fr));
    assert_eq!(parse_locale_string("de_DE.UTF-8"), Some(Language::De));
    assert_eq!(parse_locale_string("it_IT.UTF-8"), Some(Language::It));
    assert_eq!(parse_locale_string("ja_JP.UTF-8"), Some(Language::Ja));
    assert_eq!(parse_locale_string("zh_CN.UTF-8"), Some(Language::ZhCn));
    assert_eq!(parse_locale_string("C.UTF-8"), None);
    assert_eq!(parse_locale_string(""), None);

    for choice in LanguageChoice::ALL {
        let lang = match choice {
            LanguageChoice::Auto => Language::En,
            _ => choice.resolve(),
        };
        let tr = Strings::for_language(lang);
        assert!(!tr.app_title.is_empty());
        assert!(!tr.good.is_empty());
        assert!(!tr.caution.is_empty());
        assert!(!tr.bad.is_empty());
        assert!(!tr.unknown.is_empty());
        assert!(!tr.menu_file.is_empty());
        assert!(!tr.menu_theme.is_empty());
        assert!(!tr.menu_language.is_empty());
        assert!(!tr.tab_smart.is_empty());
    }
}

#[test]
fn test_themes_and_export_report() {
    for &kind in ThemeKind::ALL {
        let pal = ThemePalette::from_kind(kind, 0.9);
        assert_ne!(pal.text_primary, pal.panel_bg);
        assert_eq!(pal.is_dark, kind.is_dark());
    }

    let cfg = AppConfig::default();
    let d1 = parse_smartctl_device_json(ATA_SSD_JSON, None, None, &cfg.eval_options()).unwrap();
    let d2 = parse_smartctl_device_json(NVME_SSD_JSON, None, None, &cfg.eval_options()).unwrap();
    let tr = Strings::for_language(Language::PtBr);

    let single = format_single_disk_report(&d1, &cfg, tr);
    assert!(single.contains("CT1000BX500SSD1"));
    assert!(single.contains("(masked)"));

    let full = format_full_report(&[d1, d2], &cfg, tr);
    assert!(full.contains("Samsung SSD 990 PRO 2TB"));
    assert!(full.contains("CT1000BX500SSD1"));
}

#[test]
fn test_ata_device_statistics_override_and_failed_smart_status() {
    let json = r#"{
      "device": { "name": "/dev/sdc", "type": "sat", "protocol": "ATA" },
      "model_name": "Samsung SSD 870 EVO 1TB",
      "rotation_rate": 0,
      "logical_block_size": 512,
      "smart_status": { "passed": false },
      "ata_smart_attributes": {
        "table": [
          { "id": 241, "name": "Total_LBAs_Written", "value": 99, "worst": 99, "thresh": 0, "raw": { "value": 100, "string": "100" } }
        ]
      },
      "ata_device_statistics": {
        "pages": [
          {
            "number": 1,
            "name": "General Statistics",
            "table": [
              { "name": "Logical Sectors Written", "value": 2000000000 },
              { "name": "Logical Sectors Read", "value": 1000000000 }
            ]
          },
          {
            "number": 7,
            "name": "Solid State Device Statistics",
            "table": [
              { "name": "Percentage Used Endurance Indicator", "value": 7 }
            ]
          }
        ]
      }
    }"#;

    let disk = parse_smartctl_device_json(json, None, None, &HealthEvalOptions::default()).unwrap();
    // Device statistics override legacy attribute 241 (100 LBAs)
    assert_eq!(disk.total_host_writes_bytes, Some(2_000_000_000u128 * 512));
    assert_eq!(disk.total_host_reads_bytes, Some(1_000_000_000u128 * 512));
    assert_eq!(disk.health_percentage, Some(93));
    assert_eq!(disk.health_status, HealthStatus::Bad);
    assert_eq!(disk.ata_device_stats.len(), 2);
}

#[test]
fn test_nvme_available_spare_edge_cases() {
    let opts = HealthEvalOptions::default();

    // Case 1: spare < threshold -> Bad
    let bad_spare = NVME_SSD_JSON.replace("\"available_spare\": 100", "\"available_spare\": 5");
    let d_bad = parse_smartctl_device_json(&bad_spare, None, None, &opts).unwrap();
    assert_eq!(d_bad.health_status, HealthStatus::Bad);

    // Case 2: spare == threshold (threshold != 100) -> Caution
    let eq_spare = NVME_SSD_JSON.replace("\"available_spare\": 100", "\"available_spare\": 10");
    let d_caut = parse_smartctl_device_json(&eq_spare, None, None, &opts).unwrap();
    assert_eq!(d_caut.health_status, HealthStatus::Caution);

    // Case 3: threshold > 100 (quirky firmware) -> ignored (Good)
    let quirk = NVME_SSD_JSON.replace(
        "\"available_spare_threshold\": 10",
        "\"available_spare_threshold\": 200",
    );
    let d_ok = parse_smartctl_device_json(&quirk, None, None, &opts).unwrap();
    assert_eq!(d_ok.health_status, HealthStatus::Good);
}

#[test]
fn test_malformed_and_permission_denied_json_fallback() {
    let opts = HealthEvalOptions::default();
    assert!(parse_smartctl_device_json("not json", None, None, &opts).is_none());
    assert!(parse_scan_json("").is_empty());

    let denied_json = r#"{
      "smartctl": {
        "messages": [{ "string": "Smartctl open device: /dev/nvme0 failed: Permission denied", "severity": "error" }],
        "exit_status": 2
      }
    }"#;
    let mut disk =
        parse_smartctl_device_json(denied_json, Some("/dev/nvme0"), None, &opts).unwrap();
    assert!(!disk.smartctl_available_data);
    assert!(
        disk.smartctl_error_message
            .as_deref()
            .unwrap()
            .contains("Permission denied")
    );

    let sysfs = SysfsInfo {
        block_name: Some("nvme0n1".into()),
        lsblk_model: Some("WD Green SN350".into()),
        lsblk_serial: Some("ABC123".into()),
        lsblk_tran: Some("nvme".into()),
        lsblk_size_bytes: Some(1_000_204_886_016),
        current_link_speed: Some("8.0 GT/s PCIe".into()),
        current_link_width: Some(4),
        max_link_speed: Some("8.0 GT/s PCIe".into()),
        max_link_width: Some(4),
        rotational: Some(false),
        ..Default::default()
    };
    let part = PartitionInfo {
        name: "nvme0n1p1".into(),
        path: "/dev/nvme0n1p1".into(),
        part_type: "part".into(),
        size_bytes: 1_000_000_000_000,
        fstype: Some("ext4".into()),
        label: None,
        partlabel: None,
        uuid: Some("1111-2222".into()),
        mountpoints: vec!["/home".into()],
        fs_total_bytes: Some(980_000_000_000),
        fs_used_bytes: Some(500_000_000_000),
        fs_avail_bytes: Some(450_000_000_000),
        children: Vec::new(),
    };
    merge_sysfs_and_partitions(&mut disk, sysfs, vec![part], Vec::new());

    assert_eq!(disk.model_name, "WD Green SN350");
    assert_eq!(disk.protocol_str, "NVMe");
    assert_eq!(disk.transfer_mode, "PCIe 3.0 x4 | PCIe 3.0 x4");
    assert_eq!(disk.all_mountpoints(), vec!["/home".to_string()]);
}

#[test]
fn test_export_disk_json_roundtrip() {
    let opts = HealthEvalOptions::default();
    let disk = parse_smartctl_device_json(ATA_SSD_JSON, None, None, &opts).unwrap();
    let tmp = std::env::temp_dir().join("rdiskinfo_test_export.json");
    export_disk_json(&disk, &tmp).unwrap();
    let content = std::fs::read_to_string(&tmp).unwrap();
    let _ = std::fs::remove_file(&tmp);
    assert!(content.contains("CT1000BX500SSD1"));
}

#[test]
fn test_egui_headless_render_all_themes_and_protocols() {
    let cfg = AppConfig::default();
    let opts = cfg.eval_options();
    let ata = parse_smartctl_device_json(ATA_SSD_JSON, None, None, &opts).unwrap();
    let nvme = parse_smartctl_device_json(NVME_SSD_JSON, None, None, &opts).unwrap();
    let scsi = parse_smartctl_device_json(SCSI_DRIVE_JSON, None, None, &opts).unwrap();
    let disks: Vec<DiskInfo> = vec![ata, nvme, scsi];
    let tr = Strings::for_language(Language::PtBr);

    let ctx = egui::Context::default();
    egui_extras::install_image_loaders(&ctx);
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1080.0, 680.0),
        )),
        ..Default::default()
    };

    for &theme_kind in ThemeKind::ALL {
        let palette = ThemePalette::from_kind(theme_kind, cfg.panel_opacity);
        palette.apply_to_ctx(&ctx);

        for idx in 0..disks.len() {
            let mut sel = idx;
            let mut serial_revealed = false;
            let mut toast = None;
            let mut smart_filter = String::new();
            let mut raw_json = false;
            let mut raw_filter = String::new();
            let mut dialogs_state = DialogStates {
                grid_open: idx == 0,
                ascii_open: idx == 1,
                ascii_device: "/dev/nvme0".into(),
                ascii_hex_text: hex_dump(b"TEST_NVME_DATA_BLOCK"),
                ascii_raw_bytes: b"TEST_NVME_DATA_BLOCK".to_vec(),
                self_test_log_open: idx == 0,
                about_open: idx == 2,
                self_test_modal: if idx == 0 {
                    Some(SelfTestModalState {
                        device_node: "/dev/sda".into(),
                        device_type: "sat".into(),
                        result: SelfTestResult {
                            ok: false,
                            already_running: true,
                            outcome: SelfTestOutcome::AlreadyRunning,
                            progress_info: Some("40% remaining".into()),
                            message: "Test already running".into(),
                            raw_json: "{}".into(),
                        },
                    })
                } else {
                    None
                },
                ..Default::default()
            };

            let mut out = ctx.run_ui(input.clone(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    artwork::paint_window_background(
                        ui.painter(),
                        ui.max_rect(),
                        &palette,
                        None,
                        true,
                    );
                    if theme_kind.has_stylized_artwork() {
                        artwork::paint_sakura_mascot_card(
                            ui.painter(),
                            egui::Rect::from_min_size(
                                egui::Pos2::new(860.0, 0.0),
                                egui::vec2(210.0, 660.0),
                            ),
                            palette.is_dark,
                        );
                    }
                    disk_bar::show_disk_switcher(ui, &disks, &mut sel, &cfg, tr, &palette);
                    disk_bar::show_navigation_header(ui, &disks, &mut sel, &cfg, &palette);
                    summary_panel::show_summary_panel(
                        ui,
                        &disks[sel],
                        &cfg,
                        tr,
                        &palette,
                        &mut serial_revealed,
                        &mut toast,
                    );
                    smart_table::show_smart_table(
                        ui,
                        &disks[sel],
                        &cfg,
                        tr,
                        &palette,
                        &mut smart_filter,
                    );
                    details_tabs::show_hardware_tab(ui, &disks[sel], &cfg, tr, &palette);
                    details_tabs::show_partitions_tab(ui, &disks[sel], &cfg, tr, &palette);
                    let _ = details_tabs::show_diagnostics_tab(ui, &disks[sel], &cfg, tr, &palette);
                    let _ = details_tabs::show_raw_tab(
                        ui,
                        &disks[sel],
                        tr,
                        &palette,
                        &mut raw_json,
                        &mut raw_filter,
                    );
                });

                let _ = dialogs::show_dialogs(
                    ui.ctx(),
                    &mut dialogs_state,
                    DialogViewContext {
                        disks: &disks,
                        selected_idx: sel,
                        cfg: &cfg,
                        tr,
                        palette: &palette,
                        smartctl_version: Some("smartctl 7.5"),
                        elevation: &ElevationState::ElevatedHelper,
                    },
                );
            });
            assert!(!out.shapes.is_empty());
            out.textures_delta.clear();
        }
    }
}

#[test]
fn test_hardware_temp_thresholds_preserved_and_when_failed_handling() {
    // 1. NVMe with hardware WCTEMP=65 must NOT be overwritten by user default 75
    let nvme_65 = NVME_SSD_JSON.replace(
        "Warning  Comp. Temp. Threshold:     82 Celsius",
        "Warning  Comp. Temp. Threshold:     65 Celsius",
    );
    let mut opts = HealthEvalOptions {
        temp_warning_nvme: 75,
        temp_critical_nvme: 88,
        ..Default::default()
    };
    let mut d = parse_smartctl_device_json(&nvme_65, None, None, &opts).unwrap();
    assert_eq!(d.temperature.warning_threshold, 65);
    assert_eq!(d.temperature.critical_threshold, 85);
    evaluate_health(&mut d, &opts);
    assert_eq!(d.temperature.warning_threshold, 65);

    // 2. NVMe WITHOUT hardware WCTEMP updates dynamically when opts change
    let nvme_no_thresh = NVME_SSD_JSON
        .replace(
            "Warning  Comp. Temp. Threshold:     82 Celsius",
            "Info: none",
        )
        .replace(
            "Critical Comp. Temp. Threshold:     85 Celsius",
            "Info: none",
        );
    let mut d2 = parse_smartctl_device_json(&nvme_no_thresh, None, None, &opts).unwrap();
    assert_eq!(d2.temperature.warning_threshold, 75);
    opts.temp_warning_nvme = 68;
    evaluate_health(&mut d2, &opts);
    assert_eq!(d2.temperature.warning_threshold, 68);

    // 3. ATA with op_limit_max/limit_max preserved & when_failed="in_the_past" -> Caution with accurate text
    let ata_json = r#"{
      "device": { "name": "/dev/sda", "type": "sat", "protocol": "ATA" },
      "model_name": "TEST_ATA",
      "smart_status": { "passed": true },
      "temperature": { "current": 40, "op_limit_max": 58, "limit_max": 66 },
      "ata_smart_attributes": {
        "table": [
          { "id": 190, "name": "Airflow_Temperature_Cel", "value": 50, "worst": 35, "thresh": 45, "when_failed": "in_the_past", "raw": { "value": 40, "string": "40 (Min/Max 20/65)" } }
        ]
      }
    }"#;
    let d3 = parse_smartctl_device_json(ata_json, None, None, &opts).unwrap();
    assert_eq!(d3.temperature.warning_threshold, 58);
    assert_eq!(d3.temperature.critical_threshold, 66);
    assert_eq!(d3.health_status, HealthStatus::Caution);
    assert!(
        d3.health_reasons
            .iter()
            .any(|r| r.contains("failed: in_the_past"))
    );
    assert!(!d3.health_reasons.iter().any(|r| r.contains("50 < 45")));
}

#[test]
fn test_unprivileged_sysfs_merge_sets_protocol_standard_and_dedups_sensors() {
    let mut disk = DiskInfo {
        device_node: "/dev/nvme0".into(),
        ..Default::default()
    };
    disk.temperature.sensors.push(rdiskinfo::model::TempSensor {
        label: "Temperature Sensor 1".into(),
        celsius: 45.0,
        max_celsius: None,
        crit_celsius: None,
    });
    let sysfs = SysfsInfo {
        block_name: Some("nvme0n1".into()),
        lsblk_model: Some("WD SN350".into()),
        lsblk_tran: Some("nvme".into()),
        rotational: Some(false),
        ..Default::default()
    };
    let hwmon = vec![
        rdiskinfo::model::TempSensor {
            label: "Composite".into(),
            celsius: 46.0,
            max_celsius: Some(80.0),
            crit_celsius: Some(85.0),
        },
        rdiskinfo::model::TempSensor {
            label: "Sensor 1".into(),
            celsius: 45.0,
            max_celsius: None,
            crit_celsius: None,
        },
    ];
    merge_sysfs_and_partitions(&mut disk, sysfs, Vec::new(), hwmon);
    assert_eq!(disk.protocol, DiskProtocol::Nvme);
    assert_eq!(disk.standard, "NVM Express");
    // "Sensor 1" should be deduplicated against "Temperature Sensor 1"
    assert_eq!(disk.temperature.sensors.len(), 2);
}

#[test]
fn test_nvme_controller_name_and_bundle_matching_no_prefix_collision() {
    assert_eq!(nvme_controller_name("nvme0n1").as_deref(), Some("nvme0"));
    assert_eq!(nvme_controller_name("nvme0c1n1").as_deref(), Some("nvme0"));
    assert_eq!(nvme_controller_name("nvme10n1").as_deref(), Some("nvme10"));
    assert_eq!(nvme_controller_name("sda"), None);

    let mut bundles = std::collections::HashMap::new();
    bundles.insert(
        "/dev/nvme10".to_string(),
        BlockDeviceBundle {
            block_name: "nvme10n1".to_string(),
            smart_device_node: "/dev/nvme10".to_string(),
            partitions: Vec::new(),
            sysfs: SysfsInfo::default(),
            hwmon_sensors: Vec::new(),
        },
    );
    assert!(find_bundle_for_device(&bundles, "/dev/nvme1").is_none());
    assert!(find_bundle_for_device(&bundles, "/dev/nvme10").is_some());
    assert!(find_bundle_for_device(&bundles, "/dev/nvme10n1").is_some());
}

#[test]
fn test_nested_drivetemp_hwmon_discovery() {
    let base = std::env::temp_dir().join("rdiskinfo_test_hwmon");
    let nested = base.join("hwmon").join("hwmon4");
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(nested.join("temp1_input"), "37000\n").unwrap();
    std::fs::write(nested.join("temp1_label"), "Drive Temp\n").unwrap();
    std::fs::write(nested.join("temp1_max"), "55000\n").unwrap();

    let sensors = read_hwmon_sensors(&base);
    let _ = std::fs::remove_dir_all(&base);

    assert_eq!(sensors.len(), 1);
    assert_eq!(sensors[0].label, "Drive Temp");
    assert!((sensors[0].celsius - 37.0).abs() < 0.01);
    assert_eq!(sensors[0].max_celsius, Some(55.0));
}

#[test]
fn test_self_test_response_bitmask_handling() {
    let ok_with_errlog = r#"{
      "smartctl": {
        "exit_status": 64,
        "output": [
          "=== START OF OFFLINE IMMEDIATE AND SELF-TEST SECTION ===",
          "Sending command: \"Execute SMART Short self-test routine immediately in off-line mode\".",
          "Drive command \"Execute SMART Short self-test routine immediately in off-line mode\" successful.",
          "Testing has begun.",
          "Please wait 2 minutes for test to complete."
        ]
      }
    }"#;
    let res = parse_self_test_response(64, ok_with_errlog, "");
    assert!(res.ok);
    assert!(!res.already_running);

    let running_with_errlog = r#"{
      "smartctl": {
        "exit_status": 68,
        "output": [
          "Can't start self-test without aborting current test (40% remaining)."
        ]
      }
    }"#;
    let res2 = parse_self_test_response(68, running_with_errlog, "");
    assert!(!res2.ok);
    assert!(res2.already_running);
    assert!(res2.progress_info.as_deref().unwrap().contains("40%"));
}

#[test]
fn test_nvme_power_state_operational_flag_and_latencies() {
    let disk = parse_smartctl_device_json(NVME_SSD_JSON, None, None, &HealthEvalOptions::default())
        .unwrap();
    assert_eq!(disk.nvme_power_states.len(), 2);
    assert!(!disk.nvme_power_states[0].is_non_operational);
    assert_eq!(disk.nvme_power_states[0].max_power_watts, Some(6.5));
    assert_eq!(disk.nvme_power_states[0].entry_latency_us, Some(0));
    assert_eq!(disk.nvme_power_states[0].exit_latency_us, Some(0));
}

#[test]
fn test_locale_colon_fallback_and_full_catalog_completeness() {
    assert_eq!(
        parse_locale_string("xx_YY:pt_BR.UTF-8:en"),
        Some(Language::PtBr)
    );
    for choice in LanguageChoice::ALL {
        let lang = match choice {
            LanguageChoice::Auto => Language::En,
            _ => choice.resolve(),
        };
        let tr = Strings::for_language(lang);
        assert!(!tr.filter_attributes_hint.is_empty());
        assert!(!tr.sec_temp_sensors.is_empty());
        assert!(!tr.sec_nvme_pcie.is_empty());
        assert!(!tr.sec_kernel_queue.is_empty());
        assert!(!tr.sec_io_stats.is_empty());
        assert!(!tr.no_partitions_msg.is_empty());
        assert!(!tr.reading_raw_msg.is_empty());
        assert!(!tr.elev_root.is_empty());
    }
}

#[test]
fn test_egui_glyph_coverage_and_compact_window_render() {
    let ctx = egui::Context::default();
    let cfg = AppConfig::default();
    let opts = cfg.eval_options();
    let ata = parse_smartctl_device_json(ATA_SSD_JSON, None, None, &opts).unwrap();
    let nvme = parse_smartctl_device_json(NVME_SSD_JSON, None, None, &opts).unwrap();
    let disks = vec![ata, nvme];

    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(720.0, 520.0),
        )),
        ..Default::default()
    };

    for lang in [Language::PtBr, Language::De, Language::En] {
        let tr = Strings::for_language(lang);
        let palette = ThemePalette::from_kind(ThemeKind::CrystalBlue, 0.90);
        palette.apply_to_ctx(&ctx);
        let mut sel = 0usize;
        let mut rev = false;
        let mut toast = None;
        let mut filter = String::new();
        let mut out = ctx.run_ui(input.clone(), |ui| {
            let font_id = egui::FontId::proportional(13.0);
            ui.fonts_mut(|fonts| {
                for ch in ['🔍', '👁', '🙈', '📋', '🔒', '⚠', '◀', '▶', '✖', '•']
                {
                    assert!(
                        fonts.has_glyph(&font_id, ch),
                        "Missing bundled glyph for {:?} (U+{:04X})",
                        ch,
                        ch as u32
                    );
                }
            });
            egui::CentralPanel::default().show(ui, |ui| {
                disk_bar::show_disk_switcher(ui, &disks, &mut sel, &cfg, tr, &palette);
                disk_bar::show_navigation_header(ui, &disks, &mut sel, &cfg, &palette);
                summary_panel::show_summary_panel(
                    ui,
                    &disks[sel],
                    &cfg,
                    tr,
                    &palette,
                    &mut rev,
                    &mut toast,
                );
                smart_table::show_smart_table(ui, &disks[sel], &cfg, tr, &palette, &mut filter);
            });
        });
        assert!(!out.shapes.is_empty());
        out.textures_delta.clear();
    }
}

#[test]
fn test_raid_dev_type_validation_selftest_args_bundle_and_selection() {
    // 1. validate_dev_type safe allowlist
    assert!(validate_dev_type("").is_ok());
    assert!(validate_dev_type("sat").is_ok());
    assert!(validate_dev_type("megaraid,0").is_ok());
    assert!(validate_dev_type("areca,1/2").is_ok());
    assert!(validate_dev_type("cciss,0").is_ok());
    assert!(validate_dev_type("jmb39x,0+sat").is_ok());
    assert!(validate_dev_type("-d").is_err());
    assert!(validate_dev_type("--version").is_err());
    assert!(validate_dev_type("sat;id").is_err());
    assert!(validate_dev_type("sat space").is_err());
    assert!(validate_dev_type("a/../b").is_err());

    // 2. start/abort self-test args include -d <dev_type> unless empty or "scsi"
    assert_eq!(
        build_start_self_test_args("/dev/bus/0", "megaraid,2", "short").unwrap(),
        vec!["--json=o", "-d", "megaraid,2", "-t", "short", "/dev/bus/0"]
    );
    assert_eq!(
        build_start_self_test_args("/dev/sda", "scsi", "long").unwrap(),
        vec!["--json=o", "-t", "long", "/dev/sda"]
    );
    assert_eq!(
        build_start_self_test_args("/dev/sda", "", "conveyance").unwrap(),
        vec!["--json=o", "-t", "conveyance", "/dev/sda"]
    );
    assert_eq!(
        build_abort_self_test_args("/dev/bus/0", "megaraid,2").unwrap(),
        vec!["-d", "megaraid,2", "-X", "/dev/bus/0"]
    );
    assert_eq!(
        build_abort_self_test_args("/dev/nvme0", "nvme").unwrap(),
        vec!["-d", "nvme", "-X", "/dev/nvme0"]
    );
    assert_eq!(
        build_abort_self_test_args("/dev/sda", "scsi").unwrap(),
        vec!["-X", "/dev/sda"]
    );
    assert!(build_start_self_test_args("/dev/bus/0", "-bad", "short").is_err());
    assert!(build_abort_self_test_args("/etc/passwd", "sat").is_err());

    // 3. Avoid attaching generic block bundle to /dev/bus/N or RAID passthrough targets
    let mut bundles = std::collections::HashMap::new();
    bundles.insert(
        "/dev/sda".to_string(),
        BlockDeviceBundle {
            block_name: "sda".to_string(),
            smart_device_node: "/dev/sda".to_string(),
            partitions: Vec::new(),
            sysfs: SysfsInfo::default(),
            hwmon_sensors: Vec::new(),
        },
    );
    assert!(find_bundle_for_device(&bundles, "/dev/bus/0").is_none());
    assert!(find_bundle_for_smart_device(&bundles, "/dev/bus/0", "megaraid,0").is_none());
    assert!(find_bundle_for_smart_device(&bundles, "/dev/sda", "megaraid,0").is_none());
    assert!(find_bundle_for_smart_device(&bundles, "/dev/sda", "cciss,1").is_none());
    assert!(find_bundle_for_smart_device(&bundles, "/dev/sda", "sat").is_some());
    assert!(find_bundle_for_smart_device(&bundles, "/dev/sda", "scsi").is_some());

    // 4. Track selected disk across rescans by (device_node, device_type)
    let raid_disks = vec![
        DiskInfo {
            device_node: "/dev/bus/0".into(),
            device_type: "megaraid,0".into(),
            model_name: "RAID Disk 0".into(),
            ..Default::default()
        },
        DiskInfo {
            device_node: "/dev/bus/0".into(),
            device_type: "megaraid,1".into(),
            model_name: "RAID Disk 1".into(),
            ..Default::default()
        },
        DiskInfo {
            device_node: "/dev/sda".into(),
            device_type: "sat".into(),
            model_name: "OS SSD".into(),
            ..Default::default()
        },
    ];
    assert_eq!(
        find_selected_disk_index(&raid_disks, "/dev/bus/0", "megaraid,1"),
        1
    );
    // Unique-node fallback when unprivileged lsblk_tran transitions to smartctl dev_type
    assert_eq!(find_selected_disk_index(&raid_disks, "/dev/sda", "sata"), 2);
}

#[test]
fn test_nvme_composite_temp_kelvin_and_live_fahrenheit_export() {
    let mut cfg = AppConfig {
        use_fahrenheit: true,
        ..Default::default()
    };
    let disk = parse_smartctl_device_json(NVME_SSD_JSON, None, None, &cfg.eval_options()).unwrap();
    let comp = disk
        .nvme_attributes
        .iter()
        .find(|a| a.key == "temperature")
        .unwrap();
    // 52 °C -> 325 K == 0x0145 (CrystalDiskInfo / NVMe spec)
    assert_eq!(comp.raw_value, 325);
    assert_eq!(format!("{:04X}", comp.raw_value), "0145");
    assert_eq!(comp.decoded_value, "52 °C");
    assert!(comp.note.contains("325 K"));

    let s1 = disk
        .nvme_attributes
        .iter()
        .find(|a| a.key == "temperature_sensor_1")
        .unwrap();
    assert_eq!(s1.raw_value, 325);

    let report_f = format_single_disk_report(&disk, &cfg, Strings::for_language(Language::En));
    // 52 °C == 125.6 °F -> 126 °F in both Composite Temperature and Sensor 1
    assert!(report_f.contains("0145 126 °F"));

    cfg.use_fahrenheit = false;
    let report_c = format_single_disk_report(&disk, &cfg, Strings::for_language(Language::En));
    assert!(report_c.contains("0145 52 °C"));
}

#[test]
fn test_sysfs_4096_physical_block_size_preserved_when_smartctl_omits_it() {
    let opts = HealthEvalOptions::default();
    let mut disk = parse_smartctl_device_json(NVME_SSD_JSON, None, None, &opts).unwrap();
    // NVME_SSD_JSON specifies logical_block_size: 4096 and omits physical_block_size
    assert_eq!(disk.logical_block_size, 4096);
    assert_eq!(disk.physical_block_size, 0);

    // Also test JSON omitting both logical and physical block size
    let minimal_json = r#"{
      "device": { "name": "/dev/sda", "type": "sat", "protocol": "ATA" },
      "model_name": "AF_HDD_4K",
      "smart_status": { "passed": true }
    }"#;
    let mut disk2 = parse_smartctl_device_json(minimal_json, None, None, &opts).unwrap();
    assert_eq!(disk2.logical_block_size, 0);
    assert_eq!(disk2.physical_block_size, 0);

    let sysfs = SysfsInfo {
        block_name: Some("sda".into()),
        logical_block_size: Some(512),
        physical_block_size: Some(4096),
        ..Default::default()
    };
    merge_sysfs_and_partitions(&mut disk2, sysfs.clone(), Vec::new(), Vec::new());
    assert_eq!(disk2.logical_block_size, 512);
    assert_eq!(disk2.physical_block_size, 4096);

    merge_sysfs_and_partitions(&mut disk, sysfs, Vec::new(), Vec::new());
    assert_eq!(disk.logical_block_size, 4096);
    assert_eq!(disk.physical_block_size, 4096);

    // Without sysfs values, fallback to 512
    let mut disk3 = parse_smartctl_device_json(minimal_json, None, None, &opts).unwrap();
    merge_sysfs_and_partitions(&mut disk3, SysfsInfo::default(), Vec::new(), Vec::new());
    assert_eq!(disk3.logical_block_size, 512);
    assert_eq!(disk3.physical_block_size, 512);
}

#[test]
fn test_ata_link_speed_lookup_nvme_power_state_star_and_zero_temp_consistency() {
    // 1. find_ata_link_speed prefers <ancestor>/link*/ata_link/link*/sata_spd over class linkN
    let root = std::env::temp_dir().join("rdiskinfo_test_ata_link");
    let _ = std::fs::remove_dir_all(&root);
    let dev_canon = root.join("ata3/host2/target2:0:0/2:0:0:0");
    let direct_spd_dir = root.join("ata3/link9/ata_link/link9");
    let class_dir = root.join("class_ata_link");
    std::fs::create_dir_all(&dev_canon).unwrap();
    std::fs::create_dir_all(&direct_spd_dir).unwrap();
    std::fs::create_dir_all(class_dir.join("link3")).unwrap();
    std::fs::write(direct_spd_dir.join("sata_spd"), "6.0 Gbps\n").unwrap();
    std::fs::write(class_dir.join("link3/sata_spd"), "1.5 Gbps\n").unwrap();

    assert_eq!(
        find_ata_link_speed_with_class_dir(&dev_canon, &class_dir).as_deref(),
        Some("6.0 Gbps")
    );
    // When direct link speed is <unknown>, falls back to class/link3/sata_spd
    std::fs::write(direct_spd_dir.join("sata_spd"), "<unknown>\n").unwrap();
    assert_eq!(
        find_ata_link_speed_with_class_dir(&dev_canon, &class_dir).as_deref(),
        Some("1.5 Gbps")
    );
    let _ = std::fs::remove_dir_all(&root);

    // 2. parse_nvme_power_state_line handles * active-state marker before/after +/-
    let ps_star_op = parse_nvme_power_state_line(
        " 0 * +   6.50W       -        -    0  0  0  0        0       0",
    )
    .unwrap();
    assert_eq!(ps_star_op.state, 0);
    assert!(ps_star_op.is_active);
    assert!(!ps_star_op.is_non_operational);
    assert_eq!(ps_star_op.max_power_watts, Some(6.5));

    let ps_star_nonop = parse_nvme_power_state_line(
        " 3 * -   0.0050W     -        -    3  3  3  3     2000    2000",
    )
    .unwrap();
    assert_eq!(ps_star_nonop.state, 3);
    assert!(ps_star_nonop.is_active);
    assert!(ps_star_nonop.is_non_operational);
    assert_eq!(ps_star_nonop.entry_latency_us, Some(2000));
    assert_eq!(ps_star_nonop.exit_latency_us, Some(2000));

    let ps_op_star = parse_nvme_power_state_line(
        " 1 + *   5.80W       -        -    1  1  1  1       10      20",
    )
    .unwrap();
    assert!(ps_op_star.is_active);
    assert!(!ps_op_star.is_non_operational);

    let ps_compact = parse_nvme_power_state_line(
        " 2 *-    0.05W       -        -    2  2  2  2      100     200",
    )
    .unwrap();
    assert!(ps_compact.is_active);
    assert!(ps_compact.is_non_operational);

    // 3. 0 °C consistency between format_temperature, temp_status, and hwmon fallback
    assert_eq!(format_temperature(Some(0), false), "-- °C");
    assert_eq!(format_temperature(Some(0), true), "-- °F");
    let t0 = TemperatureInfo {
        current_celsius: Some(0),
        ..Default::default()
    };
    assert_eq!(temp_status(&t0), HealthStatus::Unknown);

    let zero_temp_json = r#"{
      "device": { "name": "/dev/sda", "type": "sat", "protocol": "ATA" },
      "model_name": "ZERO_TEMP_DRIVE",
      "temperature": { "current": 0 }
    }"#;
    let mut disk =
        parse_smartctl_device_json(zero_temp_json, None, None, &HealthEvalOptions::default())
            .unwrap();
    assert_eq!(disk.temperature.current_celsius, None);
    merge_sysfs_and_partitions(
        &mut disk,
        SysfsInfo::default(),
        Vec::new(),
        vec![rdiskinfo::model::TempSensor {
            label: "Drive Temp".into(),
            celsius: 39.0,
            max_celsius: None,
            crit_celsius: None,
        }],
    );
    assert_eq!(disk.temperature.current_celsius, Some(39));
}

#[test]
fn test_raw_ioctl_sg_io_succeeded_and_cdb_builders() {
    let mut ok_hdr = SgIoHdr::zeroed();
    ok_hdr.dxfer_len = 512;
    ok_hdr.resid = 0;
    assert!(sg_io_succeeded(0, &ok_hdr));

    // Partial read (e.g. SCSI INQUIRY returning 36 of 96 bytes) is valid
    let mut partial_hdr = SgIoHdr::zeroed();
    partial_hdr.dxfer_len = 96;
    partial_hdr.resid = 60;
    assert!(sg_io_succeeded(0, &partial_hdr));

    // Zero bytes transferred (resid == dxfer_len) must fail
    let mut empty_xfer = SgIoHdr::zeroed();
    empty_xfer.dxfer_len = 512;
    empty_xfer.resid = 512;
    assert!(!sg_io_succeeded(0, &empty_xfer));

    // Non-zero rc, status, host_status, driver_status, or SG_INFO_CHECK bit must fail
    assert!(!sg_io_succeeded(-1, &ok_hdr));

    let mut check_cond = SgIoHdr::zeroed();
    check_cond.status = 0x02;
    check_cond.dxfer_len = 512;
    assert!(!sg_io_succeeded(0, &check_cond));

    let mut status_1 = SgIoHdr::zeroed();
    status_1.status = 0x01;
    status_1.dxfer_len = 512;
    assert!(!sg_io_succeeded(0, &status_1));

    let mut host_err = SgIoHdr::zeroed();
    host_err.host_status = 0x07;
    host_err.dxfer_len = 512;
    assert!(!sg_io_succeeded(0, &host_err));

    let mut drv_err = SgIoHdr::zeroed();
    drv_err.driver_status = 0x08;
    drv_err.dxfer_len = 512;
    assert!(!sg_io_succeeded(0, &drv_err));

    let mut info_err = SgIoHdr::zeroed();
    info_err.info = 0x01;
    info_err.dxfer_len = 512;
    assert!(!sg_io_succeeded(0, &info_err));

    // CDB verification for SMART READ DATA (0xD0), SMART READ THRESHOLDS (0xD1), IDENTIFY (0xEC)
    let cdb12_thresh = build_ata_passthrough_12(0x08, 0xd1, 1, 0x4f, 0xc2, 0xb0);
    assert_eq!(
        cdb12_thresh,
        [0xa1, 0x08, 0x0e, 0xd1, 1, 0, 0x4f, 0xc2, 0, 0xb0, 0, 0]
    );

    let cdb16_thresh = build_ata_passthrough_16(0x08, 0xd1, 1, 0x4f, 0xc2, 0xb0);
    assert_eq!(
        cdb16_thresh,
        [
            0x85, 0x08, 0x0e, 0, 0xd1, 0, 1, 0, 0, 0, 0x4f, 0, 0xc2, 0, 0xb0, 0
        ]
    );

    let cdb16_ident = build_ata_passthrough_16(0x08, 0x00, 1, 0x00, 0x00, 0xec);
    assert_eq!(cdb16_ident[0], 0x85);
    assert_eq!(cdb16_ident[14], 0xec);
}

#[test]
fn test_ata_apm_aam_security_sct_and_selftest_in_progress_parsing() {
    let ata_rich_json = r#"{
      "device": { "name": "/dev/sda", "type": "sat", "protocol": "ATA" },
      "model_name": "HGST HTS721010A9E630",
      "smart_status": { "passed": true },
      "ata_apm": { "enabled": true, "level": 128 },
      "ata_aam": { "enabled": true, "level": 254, "recommended_level": 128 },
      "ata_security": {
        "string": "disabled, NOT FROZEN [SEC1]",
        "enabled": false,
        "locked": false,
        "frozen": false,
        "expired": false,
        "normal_time_minutes": 110,
        "extended_time_minutes": 112
      },
      "ata_sct_capabilities": {
        "error_recovery_control_supported": true,
        "feature_control_supported": true,
        "data_table_supported": true
      },
      "ata_sct_status": {
        "device_state": { "value": 0, "string": "Active" }
      },
      "ata_sct_temperature_history": {
        "sampling_period_minutes": 1,
        "logging_interval_minutes": 5,
        "table": [29, 30, 31, null, 33]
      },
      "ata_smart_data": {
        "self_test": {
          "status": {
            "value": 247,
            "string": "in progress, 70% remaining",
            "remaining_percent": 70
          },
          "polling_minutes": { "short": 2, "extended": 120 }
        }
      }
    }"#;

    let disk = parse_smartctl_device_json(ata_rich_json, None, None, &HealthEvalOptions::default())
        .unwrap();
    let apm = disk.ata_apm.as_ref().unwrap();
    assert!(apm.enabled);
    assert_eq!(apm.level, Some(128));

    let aam = disk.ata_aam.as_ref().unwrap();
    assert!(aam.enabled);
    assert_eq!(aam.level, Some(254));
    assert_eq!(aam.recommended_level, Some(128));

    let sec = disk.ata_security.as_ref().unwrap();
    assert!(!sec.enabled);
    assert!(!sec.frozen);
    assert_eq!(sec.normal_erase_minutes, Some(110));
    assert_eq!(sec.extended_erase_minutes, Some(112));

    let sct = disk.ata_sct.as_ref().unwrap();
    assert!(sct.error_recovery_control_supported);
    assert!(sct.feature_control_supported);
    assert!(sct.data_table_supported);
    assert_eq!(sct.device_state.as_deref(), Some("Active"));
    assert_eq!(sct.sampling_period_minutes, Some(1));
    assert_eq!(sct.logging_interval_minutes, Some(5));
    assert_eq!(sct.temperature_history.len(), 5);

    let st_exec = disk.self_test_execution.as_ref().unwrap();
    assert!(st_exec.in_progress);
    assert_eq!(st_exec.remaining_percent, Some(70));
    assert_eq!(st_exec.completed_percent, Some(30));

    // NVMe active self-test parsing
    let nvme_active = NVME_SSD_JSON.replace(
        "\"power_on_time\": { \"hours\": 50 }",
        r#""nvme_self_test_log": {
            "current_self_test_operation": { "value": 1, "string": "Short device self-test operation in progress" },
            "current_self_test_completion_percent": 45,
            "table": []
          },
          "power_on_time": { "hours": 50 }"#,
    );
    let nvme_disk =
        parse_smartctl_device_json(&nvme_active, None, None, &HealthEvalOptions::default())
            .unwrap();
    let nvme_exec = nvme_disk.self_test_execution.as_ref().unwrap();
    assert!(nvme_exec.in_progress);
    assert_eq!(nvme_exec.completed_percent, Some(45));
    assert_eq!(nvme_exec.remaining_percent, Some(55));
}

#[test]
fn test_session_temp_history_and_threshold_updates() {
    // 1. Session temperature history separation for RAID members & single-node key migration
    let mut history = std::collections::HashMap::new();
    let unpriv_disk = vec![DiskInfo {
        device_node: "/dev/sda".into(),
        device_type: "sata".into(),
        temperature: TemperatureInfo {
            current_celsius: Some(33),
            ..Default::default()
        },
        ..Default::default()
    }];
    record_temperature_samples(&mut history, &unpriv_disk, "12:00:00");
    assert_eq!(
        history
            .get(&("/dev/sda".to_string(), "sata".to_string()))
            .unwrap()
            .len(),
        1
    );

    let full_disks = vec![
        DiskInfo {
            device_node: "/dev/sda".into(),
            device_type: "sat".into(),
            temperature: TemperatureInfo {
                current_celsius: Some(35),
                ..Default::default()
            },
            ..Default::default()
        },
        DiskInfo {
            device_node: "/dev/bus/0".into(),
            device_type: "megaraid,0".into(),
            temperature: TemperatureInfo {
                current_celsius: Some(41),
                ..Default::default()
            },
            ..Default::default()
        },
        DiskInfo {
            device_node: "/dev/bus/0".into(),
            device_type: "megaraid,1".into(),
            temperature: TemperatureInfo {
                current_celsius: Some(46),
                ..Default::default()
            },
            ..Default::default()
        },
    ];
    record_temperature_samples(&mut history, &full_disks, "12:01:00");
    let sda_hist = history
        .get(&("/dev/sda".to_string(), "sat".to_string()))
        .unwrap();
    assert_eq!(sda_hist.len(), 2);
    assert_eq!(sda_hist[0].celsius, 33);
    assert_eq!(sda_hist[1].celsius, 35);

    assert_eq!(
        history
            .get(&("/dev/bus/0".to_string(), "megaraid,0".to_string()))
            .unwrap()[0]
            .celsius,
        41
    );
    assert_eq!(
        history
            .get(&("/dev/bus/0".to_string(), "megaraid,1".to_string()))
            .unwrap()[0]
            .celsius,
        46
    );

    // 2. Threshold updates & live re-evaluation
    let mut cfg = AppConfig::default();
    cfg.set_ata_temp_thresholds(45, 50);
    cfg.set_nvme_temp_thresholds(60, 55); // critical clamped >= warning
    assert_eq!(cfg.temp_warning_ata, 45);
    assert_eq!(cfg.temp_critical_ata, 50);
    assert_eq!(cfg.temp_warning_nvme, 60);
    assert_eq!(cfg.temp_critical_nvme, 60);

    let mut ata_disk =
        parse_smartctl_device_json(ATA_SSD_JSON, None, None, &cfg.eval_options()).unwrap();
    ata_disk.temperature.current_celsius = Some(48);
    assert_eq!(temp_status(&ata_disk.temperature), HealthStatus::Caution);

    cfg.set_ata_temp_thresholds(40, 46);
    evaluate_health(&mut ata_disk, &cfg.eval_options());
    assert_eq!(temp_status(&ata_disk.temperature), HealthStatus::Bad);

    // Config JSON serialization preserves thresholds
    let serialized = serde_json::to_string(&cfg).unwrap();
    let deserialized: AppConfig = serde_json::from_str(&serialized).unwrap();
    assert_eq!(deserialized.temp_warning_ata, 40);
    assert_eq!(deserialized.temp_critical_ata, 46);
    assert_eq!(deserialized.temp_warning_nvme, 60);
    assert_eq!(deserialized.temp_critical_nvme, 60);

    assert_eq!(format_scsi_level("6"), "6 (SPC-4)");
}

#[test]
fn test_all_eight_languages_complete_and_localized_txt_reports() {
    let cfg = AppConfig::default();
    let ata = parse_smartctl_device_json(ATA_SSD_JSON, None, None, &cfg.eval_options()).unwrap();
    let nvme = parse_smartctl_device_json(NVME_SSD_JSON, None, None, &cfg.eval_options()).unwrap();
    let scsi =
        parse_smartctl_device_json(SCSI_DRIVE_JSON, None, None, &cfg.eval_options()).unwrap();
    let disks = vec![ata.clone(), nvme, scsi];

    let all_langs = [
        Language::En,
        Language::PtBr,
        Language::Es,
        Language::Fr,
        Language::De,
        Language::It,
        Language::Ja,
        Language::ZhCn,
    ];
    let en_tr = Strings::for_language(Language::En);

    for lang in all_langs {
        let tr = Strings::for_language(lang);
        for (field, value) in tr.all_field_values() {
            assert!(
                !value.trim().is_empty(),
                "Empty translation field '{}' for {:?}",
                field,
                lang
            );
        }
        for &kind in ThemeKind::ALL {
            assert!(!kind.localized_name(tr).trim().is_empty());
        }
        assert!(!LanguageChoice::Auto.localized_label(tr).trim().is_empty());

        if lang != Language::En {
            assert_ne!(
                LanguageChoice::Auto.localized_label(tr),
                LanguageChoice::Auto.localized_label(en_tr),
                "Expected localized Auto label for {:?}",
                lang
            );
        }

        let single = format_single_disk_report(&ata, &cfg, tr);
        assert!(single.contains(tr.export_model));
        assert!(single.contains(tr.health_status));
        assert!(single.contains(tr.temperature));

        let full = format_full_report(&disks, &cfg, tr);
        assert!(full.contains(tr.export_report_title));
        assert!(full.contains(tr.export_disk_list));
    }

    // Verify Portuguese report uses Portuguese field labels (not English ones)
    let pt_full = format_full_report(&disks, &cfg, Strings::for_language(Language::PtBr));
    assert!(pt_full.contains("Relatório RDiskInfo"));
    assert!(pt_full.contains("Lista de Discos"));
    assert!(pt_full.contains("Status de Saúde"));
    assert!(!pt_full.contains("-- Disk List"));
    assert!(!pt_full.contains("Health Status :"));
}

#[test]
fn test_egui_headless_new_panels_and_threshold_modal() {
    let mut cfg = AppConfig::default();
    let opts = cfg.eval_options();
    let ata_rich = r#"{
      "device": { "name": "/dev/sda", "type": "sat", "protocol": "ATA" },
      "model_name": "TEST_RICH_ATA",
      "smart_status": { "passed": true },
      "ata_apm": { "enabled": true, "level": 128 },
      "ata_aam": { "enabled": false },
      "ata_security": { "string": "disabled, NOT FROZEN", "enabled": false, "locked": false, "frozen": false, "expired": false },
      "ata_sct_capabilities": { "error_recovery_control_supported": true, "feature_control_supported": true, "data_table_supported": true },
      "ata_sct_temperature_history": { "sampling_period_minutes": 1, "logging_interval_minutes": 1, "table": [30, 31, 32] },
      "ata_smart_data": { "self_test": { "status": { "value": 245, "string": "in progress, 50% remaining", "remaining_percent": 50 }, "polling_minutes": { "short": 2 } } }
    }"#;
    let disk = parse_smartctl_device_json(ata_rich, None, None, &opts).unwrap();
    let history = vec![
        TempSample {
            label: "10:00:00".into(),
            celsius: 31,
        },
        TempSample {
            label: "10:05:00".into(),
            celsius: 34,
        },
    ];

    let ctx = egui::Context::default();
    let tr = Strings::for_language(Language::PtBr);
    let palette = ThemePalette::from_kind(ThemeKind::CrystalBlue, 0.90);
    palette.apply_to_ctx(&ctx);

    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1000.0, 680.0),
        )),
        ..Default::default()
    };

    let mut open = true;
    let mut out = ctx.run_ui(input, |ui| {
        egui::CentralPanel::default().show(ui, |ui| {
            details_tabs::show_hardware_tab(ui, &disk, &cfg, tr, &palette);
            let _ = details_tabs::show_diagnostics_tab_with_history(
                ui, &disk, &history, &cfg, tr, &palette,
            );
        });
        let _ =
            dialogs::show_threshold_settings_dialog(ui.ctx(), &mut open, &mut cfg, tr, &palette);
    });
    assert!(!out.shapes.is_empty());
    out.textures_delta.clear();
}

#[test]
fn test_raid_scan_targets_composite_dev_type_and_fallback_preservation() {
    assert!(is_raid_controller_dev_type("megaraid,0"));
    assert!(is_raid_controller_dev_type("sat+megaraid,0"));
    assert!(is_raid_controller_dev_type("scsi+cciss,1"));
    assert!(!is_raid_controller_dev_type("sat"));
    assert!(!is_raid_controller_dev_type("scsi"));

    let scanned = vec![
        ScannedDevice {
            name: "/dev/bus/0".into(),
            info_name: "/dev/bus/0 [megaraid_disk_00]".into(),
            dev_type: "megaraid,0".into(),
            protocol: "ATA".into(),
        },
        ScannedDevice {
            name: "/dev/bus/0".into(),
            info_name: "/dev/bus/0 [megaraid_disk_01]".into(),
            dev_type: "megaraid,1".into(),
            protocol: "ATA".into(),
        },
        ScannedDevice {
            name: "/dev/nvme0n1".into(),
            info_name: "/dev/nvme0n1".into(),
            dev_type: "nvme".into(),
            protocol: "NVMe".into(),
        },
    ];
    let extra = vec![
        "/dev/bus/0".to_string(),
        "/dev/nvme0".to_string(),
        "/dev/sda".to_string(),
    ];
    let targets = collect_scan_targets(scanned, &extra);
    // Must NEVER add a bare ("/dev/bus/0", "") target, and must deduplicate /dev/nvme0 vs /dev/nvme0n1
    assert_eq!(
        targets,
        vec![
            ("/dev/bus/0".to_string(), "megaraid,0".to_string()),
            ("/dev/bus/0".to_string(), "megaraid,1".to_string()),
            ("/dev/nvme0n1".to_string(), "nvme".to_string()),
            ("/dev/sda".to_string(), String::new()),
        ]
    );

    // If smartctl JSON downgrades device.type from megaraid,0 to sat, preserve RAID fallback_type
    let json = r#"{
      "device": { "name": "/dev/bus/0", "type": "sat", "protocol": "ATA" },
      "model_name": "RAID_MEMBER",
      "smart_status": { "passed": true }
    }"#;
    let disk = parse_smartctl_device_json(
        json,
        Some("/dev/bus/0"),
        Some("megaraid,2"),
        &HealthEvalOptions::default(),
    )
    .unwrap();
    assert_eq!(disk.device_type, "megaraid,2");
}

#[test]
fn test_ata_security_bitmask_unknown_temp_thresholds_and_selftest_error_distinction() {
    // 1. ata_security state bitmask fallback & string device_state + text-only remaining_percent
    let json = r#"{
      "device": { "name": "/dev/sda", "type": "sat", "protocol": "ATA" },
      "model_name": "BITMASK_TEST",
      "smart_status": { "passed": true },
      "ata_security": { "state": 14, "string": "ENABLED, LOCKED, FROZEN" },
      "ata_sct_status": { "device_state": "Active" },
      "ata_smart_data": {
        "self_test": {
          "status": { "string": "in progress, 60% remaining" }
        }
      }
    }"#;
    let d = parse_smartctl_device_json(json, None, None, &HealthEvalOptions::default()).unwrap();
    assert!(d.features.contains(&"Security".to_string()));
    let sec = d.ata_security.as_ref().unwrap();
    assert!(sec.enabled);
    assert!(sec.locked);
    assert!(sec.frozen);
    assert!(!sec.expired);
    assert_eq!(
        d.ata_sct.as_ref().unwrap().device_state.as_deref(),
        Some("Active")
    );
    let exec = d.self_test_execution.as_ref().unwrap();
    assert!(exec.in_progress);
    assert_eq!(exec.remaining_percent, Some(60));
    assert_eq!(exec.completed_percent, Some(40));

    // 2. DiskProtocol::Unknown (e.g. unprivileged USB disk) updates temperature thresholds cleanly
    let mut unk = DiskInfo {
        device_node: "/dev/sdb".into(),
        protocol: DiskProtocol::Unknown,
        temperature: TemperatureInfo {
            current_celsius: Some(47),
            ..Default::default()
        },
        ..Default::default()
    };
    let opts = HealthEvalOptions {
        temp_warning_ata: 45,
        temp_critical_ata: 50,
        ..Default::default()
    };
    evaluate_health(&mut unk, &opts);
    assert_eq!(unk.temperature.warning_threshold, 45);
    assert_eq!(unk.temperature.critical_threshold, 50);
    assert_eq!(temp_status(&unk.temperature), HealthStatus::Caution);

    // 3. smartctl exit_status=4 with "not supported" must be Failed, not AlreadyRunning
    let unsupp = r#"{
      "smartctl": {
        "exit_status": 4,
        "output": ["Execute SMART Short self-test routine failed: Self-test not supported"]
      }
    }"#;
    let res = parse_self_test_response(4, unsupp, "");
    assert_eq!(res.outcome, SelfTestOutcome::Failed);
    assert!(!res.already_running);

    // 4. ElevationState summary formats cleanly without Rust Debug braces
    let denied = ElevationState::Denied {
        message: "Authentication cancelled".into(),
    };
    assert_eq!(denied.summary(), "Authentication cancelled");
    assert!(!denied.to_string().contains("Denied {"));
}
