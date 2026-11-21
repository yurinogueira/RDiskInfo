use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum HealthStatus {
    Good,
    Caution,
    Bad,
    #[default]
    Unknown,
}

impl HealthStatus {
    pub fn as_key(self) -> &'static str {
        match self {
            HealthStatus::Good => "Good",
            HealthStatus::Caution => "Caution",
            HealthStatus::Bad => "Bad",
            HealthStatus::Unknown => "Unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DiskProtocol {
    Ata,
    Nvme,
    Scsi,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TempSensor {
    pub label: String,
    pub celsius: f64,
    pub max_celsius: Option<f64>,
    pub crit_celsius: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TemperatureInfo {
    pub current_celsius: Option<i32>,
    pub power_cycle_min: Option<i32>,
    pub power_cycle_max: Option<i32>,
    pub lifetime_min: Option<i32>,
    pub lifetime_max: Option<i32>,
    pub op_limit_min: Option<i32>,
    pub op_limit_max: Option<i32>,
    pub limit_min: Option<i32>,
    pub limit_max: Option<i32>,
    pub drive_warning_threshold: Option<i32>,
    pub drive_critical_threshold: Option<i32>,
    pub warning_threshold: i32,
    pub critical_threshold: i32,
    pub sensors: Vec<TempSensor>,
}

impl Default for TemperatureInfo {
    fn default() -> Self {
        Self {
            current_celsius: None,
            power_cycle_min: None,
            power_cycle_max: None,
            lifetime_min: None,
            lifetime_max: None,
            op_limit_min: None,
            op_limit_max: None,
            limit_min: None,
            limit_max: None,
            drive_warning_threshold: None,
            drive_critical_threshold: None,
            warning_threshold: 55,
            critical_threshold: 60,
            sensors: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartAttribute {
    pub id: u8,
    pub name: String,
    pub value: u8,
    pub worst: u8,
    pub thresh: u8,
    pub when_failed: String,
    pub flags_short: String,
    pub prefailure: bool,
    pub updated_online: bool,
    pub raw_value: u64,
    pub raw_string: String,
    pub status: HealthStatus,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NvmeSmartAttribute {
    pub id: u8,
    pub key: String,
    pub name: String,
    pub raw_value: u128,
    pub decoded_value: String,
    pub status: HealthStatus,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScsiErrorCounterRow {
    pub operation: String,
    pub ecc_fast: u64,
    pub ecc_delayed: u64,
    pub rereads_rewrites: u64,
    pub total_corrected: u64,
    pub algorithm_invocations: u64,
    pub gigabytes_processed: String,
    pub total_uncorrected: u64,
    pub status: HealthStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfTestEntry {
    pub num: Option<u32>,
    pub test_type: String,
    pub status: String,
    pub lifetime_hours: Option<u64>,
    pub lba: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorLogEntry {
    pub error_number: Option<u64>,
    pub lifetime_hours: Option<u64>,
    pub state_or_status: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevStatEntry {
    pub name: String,
    pub value: String,
    pub raw_hex: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevStatPage {
    pub page_number: u8,
    pub page_name: String,
    pub entries: Vec<DevStatEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhyEventCounter {
    pub id: u16,
    pub name: String,
    pub size: u16,
    pub value: u64,
    pub overflow: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NvmeNamespaceInfo {
    pub id: u32,
    pub size_bytes: Option<u64>,
    pub capacity_bytes: Option<u64>,
    pub utilization_bytes: Option<u64>,
    pub formatted_lba_size: Option<u32>,
    pub eui64: Option<String>,
    #[serde(default)]
    pub nguid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NvmePowerStateInfo {
    pub state: u32,
    pub max_power_watts: Option<f64>,
    pub is_non_operational: bool,
    #[serde(default)]
    pub is_active: bool,
    pub entry_latency_us: Option<u64>,
    pub exit_latency_us: Option<u64>,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AtaApmInfo {
    pub enabled: bool,
    pub level: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AtaAamInfo {
    pub enabled: bool,
    pub level: Option<u32>,
    pub recommended_level: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AtaSecurityInfo {
    pub state_string: Option<String>,
    pub enabled: bool,
    pub locked: bool,
    pub frozen: bool,
    pub expired: bool,
    pub normal_erase_minutes: Option<u32>,
    pub extended_erase_minutes: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AtaSctInfo {
    pub error_recovery_control_supported: bool,
    pub feature_control_supported: bool,
    pub data_table_supported: bool,
    pub device_state: Option<String>,
    pub sampling_period_minutes: Option<u32>,
    pub logging_interval_minutes: Option<u32>,
    pub temperature_history: Vec<Option<i32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct SelfTestExecutionStatus {
    pub in_progress: bool,
    pub status_text: String,
    pub remaining_percent: Option<u8>,
    pub completed_percent: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TempSample {
    pub label: String,
    pub celsius: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionInfo {
    pub name: String,
    pub path: String,
    pub part_type: String,
    pub size_bytes: u64,
    pub fstype: Option<String>,
    pub label: Option<String>,
    pub partlabel: Option<String>,
    pub uuid: Option<String>,
    pub mountpoints: Vec<String>,
    pub fs_total_bytes: Option<u64>,
    pub fs_used_bytes: Option<u64>,
    pub fs_avail_bytes: Option<u64>,
    pub children: Vec<PartitionInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlockIoStats {
    pub reads_completed: u64,
    pub reads_merged: u64,
    pub sectors_read: u64,
    pub read_time_ms: u64,
    pub writes_completed: u64,
    pub writes_merged: u64,
    pub sectors_written: u64,
    pub write_time_ms: u64,
    pub io_in_flight: u64,
    pub io_time_ms: u64,
    pub weighted_io_time_ms: u64,
    pub discards_completed: u64,
    pub sectors_discarded: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SysfsInfo {
    pub block_name: Option<String>,
    pub pci_bdf: Option<String>,
    pub pci_vendor_id: Option<String>,
    pub pci_device_id: Option<String>,
    pub pci_subsystem_vendor: Option<String>,
    pub pci_subsystem_device: Option<String>,
    pub current_link_speed: Option<String>,
    pub current_link_width: Option<u32>,
    pub max_link_speed: Option<String>,
    pub max_link_width: Option<u32>,
    pub numa_node: Option<i32>,
    pub nvme_transport: Option<String>,
    pub nvme_state: Option<String>,
    pub nvme_cntlid: Option<String>,
    pub nvme_subsysnqn: Option<String>,
    pub nvme_queue_count: Option<u32>,
    pub nvme_sqsize: Option<u32>,
    pub nvme_hmb: Option<String>,
    pub scheduler: Option<String>,
    pub rotational: Option<bool>,
    pub logical_block_size: Option<u64>,
    pub physical_block_size: Option<u64>,
    pub hw_sector_size: Option<u64>,
    pub minimum_io_size: Option<u64>,
    pub optimal_io_size: Option<u64>,
    pub max_sectors_kb: Option<u64>,
    pub read_ahead_kb: Option<u64>,
    pub nr_requests: Option<u64>,
    pub discard_max_bytes: Option<u64>,
    pub discard_granularity: Option<u64>,
    pub write_cache: Option<String>,
    pub fua: Option<bool>,
    pub zoned: Option<String>,
    pub ncq_prio_supported: Option<bool>,
    pub queue_depth: Option<u32>,
    pub wwid: Option<String>,
    pub sata_link_speed: Option<String>,
    pub io_stats: Option<BlockIoStats>,
    pub removable: bool,
    pub read_only: bool,
    pub hotplug: bool,
    pub lsblk_model: Option<String>,
    pub lsblk_serial: Option<String>,
    pub lsblk_firmware: Option<String>,
    pub lsblk_tran: Option<String>,
    pub lsblk_size_bytes: Option<u64>,
    #[serde(default)]
    pub vendor: Option<String>,
    #[serde(default)]
    pub scsi_level: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiskInfo {
    pub device_node: String,
    pub block_device_node: Option<String>,
    pub device_type: String,
    pub protocol: DiskProtocol,
    pub protocol_str: String,
    pub interface_label: String,
    pub transfer_mode: String,
    pub standard: String,
    pub features: Vec<String>,
    pub model_family: Option<String>,
    pub model_name: String,
    pub serial_number: String,
    pub firmware_version: String,
    pub wwn: Option<String>,
    pub form_factor: Option<String>,
    pub capacity_bytes: u64,
    pub logical_block_size: u64,
    pub physical_block_size: u64,
    pub rotation_rate: Option<u32>,
    pub power_on_hours: Option<u64>,
    pub power_on_minutes: Option<u64>,
    pub power_cycle_count: Option<u64>,
    pub unsafe_shutdowns: Option<u64>,
    pub total_host_reads_bytes: Option<u128>,
    pub total_host_writes_bytes: Option<u128>,
    pub total_nand_writes_bytes: Option<u128>,
    pub health_status: HealthStatus,
    pub health_percentage: Option<u8>,
    pub health_reasons: Vec<String>,
    pub smart_passed: Option<bool>,
    pub smart_supported: Option<bool>,
    pub smart_enabled: Option<bool>,
    pub temperature: TemperatureInfo,
    pub ata_attributes: Vec<SmartAttribute>,
    pub nvme_attributes: Vec<NvmeSmartAttribute>,
    pub scsi_error_log: Vec<ScsiErrorCounterRow>,
    pub scsi_grown_defect_list: Option<u64>,
    pub self_test_polling_minutes: Vec<(String, u32)>,
    pub self_test_log: Vec<SelfTestEntry>,
    pub nvme_self_test_supported: bool,
    pub error_log_count: Option<u64>,
    pub error_logs: Vec<ErrorLogEntry>,
    pub ata_device_stats: Vec<DevStatPage>,
    pub sata_phy_counters: Vec<PhyEventCounter>,
    pub nvme_namespaces: Vec<NvmeNamespaceInfo>,
    pub nvme_power_states: Vec<NvmePowerStateInfo>,
    pub nvme_pci_vendor_id: Option<u16>,
    pub nvme_ieee_oui: Option<u32>,
    pub nvme_controller_id: Option<u32>,
    pub nvme_total_capacity_bytes: Option<u128>,
    pub nvme_unallocated_capacity_bytes: Option<u128>,
    pub partitions: Vec<PartitionInfo>,
    pub sysfs: SysfsInfo,
    pub smartctl_available_data: bool,
    pub smartctl_error_message: Option<String>,
    pub raw_smartctl_output: Vec<String>,
    pub raw_smartctl_json: Option<String>,
    #[serde(default)]
    pub ata_apm: Option<AtaApmInfo>,
    #[serde(default)]
    pub ata_aam: Option<AtaAamInfo>,
    #[serde(default)]
    pub ata_security: Option<AtaSecurityInfo>,
    #[serde(default)]
    pub ata_sct: Option<AtaSctInfo>,
    #[serde(default)]
    pub self_test_execution: Option<SelfTestExecutionStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthEvalOptions {
    pub ignore_c4: bool,
    pub ssd_wear_warning_threshold_percent: u8,
    pub temp_warning_ata: i32,
    pub temp_critical_ata: i32,
    pub temp_warning_nvme: i32,
    pub temp_critical_nvme: i32,
}

impl Default for HealthEvalOptions {
    fn default() -> Self {
        Self {
            ignore_c4: true,
            ssd_wear_warning_threshold_percent: 10,
            temp_warning_ata: 55,
            temp_critical_ata: 60,
            temp_warning_nvme: 65,
            temp_critical_nvme: 70,
        }
    }
}

impl DiskInfo {
    /// Collect all primary mountpoints across partitions & nested RAID/LUKS children.
    pub fn all_mountpoints(&self) -> Vec<String> {
        fn collect(parts: &[PartitionInfo], out: &mut Vec<String>) {
            for p in parts {
                for m in &p.mountpoints {
                    if !m.is_empty() && !out.contains(m) {
                        out.push(m.clone());
                    }
                }
                collect(&p.children, out);
            }
        }
        let mut out = Vec::new();
        collect(&self.partitions, &mut out);
        out
    }

    pub fn display_title(&self) -> String {
        if self.model_name.is_empty() {
            self.device_node.clone()
        } else {
            self.model_name.clone()
        }
    }

    pub fn short_mount_label(&self) -> Option<String> {
        let mounts = self.all_mountpoints();
        if mounts.is_empty() {
            None
        } else if mounts.len() == 1 {
            Some(mounts[0].clone())
        } else {
            Some(format!("{} (+{})", mounts[0], mounts.len() - 1))
        }
    }
}

pub fn format_capacity(bytes: u64, use_gb_instead_of_tb: bool) -> String {
    if bytes == 0 {
        return "----".to_string();
    }
    let val = bytes as f64;
    if val < 1e6 {
        format!("{} B", bytes)
    } else if val < 1e9 {
        format!("{:.0} MB", val / 1e6)
    } else if val < 1e12 || use_gb_instead_of_tb {
        if use_gb_instead_of_tb && val >= 1e12 {
            format!("{:.1} GB", val / 1e9)
        } else {
            format!("{:.0} GB", val / 1e9)
        }
    } else if val < 1e15 {
        let tb = val / 1e12;
        if (tb - tb.round()).abs() < 0.05 {
            format!("{:.0} TB", tb)
        } else {
            format!("{:.1} TB", tb)
        }
    } else {
        format!("{:.1} PB", val / 1e15)
    }
}

pub fn format_data_volume(bytes: Option<u128>, use_gb_instead_of_tb: bool) -> String {
    let Some(bytes) = bytes else {
        return "----".to_string();
    };
    if bytes == 0 {
        return "----".to_string();
    }
    let mb = (bytes as f64) / 1e6;
    if mb < 1000.0 {
        format!("{:.0} MB", mb)
    } else if mb < 1_000_000.0 || use_gb_instead_of_tb {
        let precision = if use_gb_instead_of_tb { 1 } else { 2 };
        format!("{:.*} GB", precision, mb / 1e3)
    } else if mb < 1_000_000_000.0 {
        format!("{:.2} TB", mb / 1e6)
    } else {
        format!("{:.2} PB", mb / 1e9)
    }
}

pub fn format_temperature(celsius: Option<i32>, use_fahrenheit: bool) -> String {
    match celsius {
        Some(c) if c > 0 && c < 300 => {
            if use_fahrenheit {
                let f = ((c as f64) * 9.0 / 5.0 + 32.0).round() as i32;
                format!("{} °F", f)
            } else {
                format!("{} °C", c)
            }
        }
        _ => {
            if use_fahrenheit {
                "-- °F".to_string()
            } else {
                "-- °C".to_string()
            }
        }
    }
}

pub fn temp_status(temp: &TemperatureInfo) -> HealthStatus {
    match temp.current_celsius {
        None => HealthStatus::Unknown,
        Some(c) if c <= 0 || c >= 300 => HealthStatus::Unknown,
        Some(c) if c >= temp.critical_threshold => HealthStatus::Bad,
        Some(c) if c >= temp.warning_threshold => HealthStatus::Caution,
        Some(_) => HealthStatus::Good,
    }
}

pub fn format_scsi_level(level: &str) -> String {
    let trimmed = level.trim();
    match trimmed {
        "1" => "1 (SCSI-1)".to_string(),
        "2" => "2 (SCSI-2)".to_string(),
        "3" => "3 (SPC)".to_string(),
        "4" => "4 (SPC-2)".to_string(),
        "5" => "5 (SPC-3)".to_string(),
        "6" => "6 (SPC-4)".to_string(),
        "7" => "7 (SPC-5)".to_string(),
        other => other.to_string(),
    }
}
