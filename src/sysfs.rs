use crate::model::{BlockIoStats, PartitionInfo, SysfsInfo, TempSensor};
use serde_json::Value;
use std::collections::HashMap;
use std::ffi::CString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct BlockDeviceBundle {
    pub block_name: String,
    pub smart_device_node: String,
    pub partitions: Vec<PartitionInfo>,
    pub sysfs: SysfsInfo,
    pub hwmon_sensors: Vec<TempSensor>,
}

pub fn collect_block_devices() -> HashMap<String, BlockDeviceBundle> {
    let mut map = HashMap::new();
    let lsblk_json = run_lsblk();
    let parsed = parse_lsblk_json(&lsblk_json);

    for entry in parsed {
        let block_name = entry.name.clone();
        if is_virtual_block(&block_name, &entry.part_type) {
            continue;
        }
        let smart_device_node = resolve_smart_device_node(&block_name);
        let (sysfs, hwmon_sensors) = read_sysfs_for_block(&block_name, entry.sysfs_base);

        let bundle = BlockDeviceBundle {
            block_name,
            smart_device_node: smart_device_node.clone(),
            partitions: entry.partitions,
            sysfs,
            hwmon_sensors,
        };
        map.entry(smart_device_node)
            .and_modify(|existing: &mut BlockDeviceBundle| {
                existing.partitions.extend(bundle.partitions.clone());
            })
            .or_insert(bundle);
    }

    // Fallback if lsblk wasn't available or missed a block device
    if map.is_empty()
        && let Ok(entries) = fs::read_dir("/sys/block")
    {
        for ent in entries.flatten() {
            let name = ent.file_name().to_string_lossy().to_string();
            if is_virtual_block(&name, "disk") {
                continue;
            }
            let smart_device_node = resolve_smart_device_node(&name);
            let (sysfs, hwmon_sensors) = read_sysfs_for_block(&name, SysfsInfo::default());
            map.insert(
                smart_device_node.clone(),
                BlockDeviceBundle {
                    block_name: name,
                    smart_device_node,
                    partitions: Vec::new(),
                    sysfs,
                    hwmon_sensors,
                },
            );
        }
    }

    map
}

pub fn is_raid_controller_dev_type(dev_type: &str) -> bool {
    let lower = dev_type.trim().to_ascii_lowercase();
    const RAID_PREFIXES: &[&str] = &[
        "megaraid", "3ware", "areca", "cciss", "aacraid", "hpt", "sssraid",
    ];
    lower.split('+').any(|seg| {
        let seg = seg.trim();
        RAID_PREFIXES.iter().any(|&p| {
            seg == p || seg.starts_with(&format!("{},", p)) || seg.starts_with(&format!("{}/", p))
        })
    })
}

/// Match a smartctl device node (`/dev/nvme0`, `/dev/nvme0n1`, `/dev/sda`) to our bundle map.
pub fn find_bundle_for_device<'a>(
    bundles: &'a HashMap<String, BlockDeviceBundle>,
    device_node: &str,
) -> Option<&'a BlockDeviceBundle> {
    if device_node.starts_with("/dev/bus/") || !device_node.starts_with("/dev/") {
        return None;
    }
    if let Some(b) = bundles.get(device_node) {
        return Some(b);
    }
    let short = device_node.rsplit('/').next().unwrap_or(device_node);
    let ctrl_short = nvme_controller_name(short);
    for b in bundles.values() {
        if b.block_name == short || b.smart_device_node == device_node {
            return Some(b);
        }
        if b.smart_device_node.rsplit('/').next() == Some(short) {
            return Some(b);
        }
        if let Some(ref c) = ctrl_short
            && nvme_controller_name(&b.block_name).as_deref() == Some(c.as_str())
        {
            return Some(b);
        }
    }
    None
}

/// Match a smartctl target `(device_node, dev_type)` to a block device bundle while
/// refusing to attach a generic block bundle to RAID controller passthrough targets
/// or `/dev/bus/*` character nodes.
pub fn find_bundle_for_smart_device<'a>(
    bundles: &'a HashMap<String, BlockDeviceBundle>,
    device_node: &str,
    dev_type: &str,
) -> Option<&'a BlockDeviceBundle> {
    if device_node.starts_with("/dev/bus/") || is_raid_controller_dev_type(dev_type) {
        return None;
    }
    find_bundle_for_device(bundles, device_node)
}

fn is_virtual_block(name: &str, dev_type: &str) -> bool {
    name.starts_with("zram")
        || name.starts_with("loop")
        || name.starts_with("ram")
        || name.starts_with("dm-")
        || name.starts_with("md")
        || name.starts_with("sr")
        || name.starts_with("nbd")
        || (name.starts_with("fd") && name[2..].chars().all(|c| c.is_ascii_digit()))
        || (name.starts_with("mmcblk") && (name.contains("boot") || name.contains("rpmb")))
        || dev_type == "rom"
        || dev_type == "loop"
}

pub fn nvme_controller_name(block_name: &str) -> Option<String> {
    let rest = block_name.strip_prefix("nvme")?;
    let digits_len = rest.bytes().take_while(|b| b.is_ascii_digit()).count();
    if digits_len == 0 {
        return None;
    }
    Some(block_name[..4 + digits_len].to_string())
}

fn resolve_smart_device_node(block_name: &str) -> String {
    if let Some(ctrl) = nvme_controller_name(block_name) {
        return format!("/dev/{}", ctrl);
    }
    format!("/dev/{}", block_name)
}

fn run_lsblk() -> String {
    let out = Command::new("lsblk")
        .env("LC_ALL", "C")
        .args([
            "-J",
            "-b",
            "-o",
            "NAME,TYPE,SIZE,MOUNTPOINTS,FSTYPE,MODEL,SERIAL,ROTA,TRAN,RM,RO,HOTPLUG,UUID,PARTLABEL,LABEL",
        ])
        .output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => String::new(),
    }
}

struct LsblkDisk {
    name: String,
    part_type: String,
    partitions: Vec<PartitionInfo>,
    sysfs_base: SysfsInfo,
}

fn parse_lsblk_json(json_str: &str) -> Vec<LsblkDisk> {
    let Ok(val) = serde_json::from_str::<Value>(json_str) else {
        return Vec::new();
    };
    let Some(arr) = val.get("blockdevices").and_then(|v| v.as_array()) else {
        return Vec::new();
    };

    arr.iter()
        .filter_map(|dev| {
            let name = dev.get("name")?.as_str()?.to_string();
            let part_type = dev
                .get("type")
                .and_then(|s| s.as_str())
                .unwrap_or("disk")
                .to_string();
            let sysfs_base = SysfsInfo {
                block_name: Some(name.clone()),
                lsblk_model: dev
                    .get("model")
                    .and_then(|s| s.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string),
                lsblk_serial: dev
                    .get("serial")
                    .and_then(|s| s.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string),
                lsblk_tran: dev
                    .get("tran")
                    .and_then(|s| s.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string),
                lsblk_size_bytes: dev.get("size").and_then(json_size_u64),
                rotational: dev.get("rota").and_then(|b| b.as_bool()),
                removable: dev.get("rm").and_then(|b| b.as_bool()).unwrap_or(false),
                read_only: dev.get("ro").and_then(|b| b.as_bool()).unwrap_or(false),
                hotplug: dev
                    .get("hotplug")
                    .and_then(|b| b.as_bool())
                    .unwrap_or(false),
                ..Default::default()
            };

            let mut partitions = parse_children(dev.get("children"));
            // Handle whole-disk formatted without partition table
            let top_mounts = parse_mountpoints(dev);
            if partitions.is_empty()
                && (!top_mounts.is_empty() || dev.get("fstype").and_then(|x| x.as_str()).is_some())
            {
                partitions.push(build_partition_info(dev, Vec::new()));
            }

            Some(LsblkDisk {
                name,
                part_type,
                partitions,
                sysfs_base,
            })
        })
        .collect()
}

fn parse_children(children_val: Option<&Value>) -> Vec<PartitionInfo> {
    let Some(arr) = children_val.and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    arr.iter()
        .map(|node| {
            let sub = parse_children(node.get("children"));
            build_partition_info(node, sub)
        })
        .collect()
}

fn build_partition_info(node: &Value, children: Vec<PartitionInfo>) -> PartitionInfo {
    let name = node
        .get("name")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();
    let path = format!("/dev/{}", name);
    let mountpoints = parse_mountpoints(node);

    let (fs_total_bytes, fs_used_bytes, fs_avail_bytes) = mountpoints
        .iter()
        .find(|m| m.starts_with('/'))
        .and_then(|mp| query_statvfs(mp))
        .map(|(t, u, a)| (Some(t), Some(u), Some(a)))
        .unwrap_or((None, None, None));

    PartitionInfo {
        name,
        path,
        part_type: node
            .get("type")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string(),
        size_bytes: node.get("size").and_then(json_size_u64).unwrap_or(0),
        fstype: node
            .get("fstype")
            .and_then(|s| s.as_str())
            .map(str::to_string),
        label: node
            .get("label")
            .and_then(|s| s.as_str())
            .map(str::to_string),
        partlabel: node
            .get("partlabel")
            .and_then(|s| s.as_str())
            .map(str::to_string),
        uuid: node
            .get("uuid")
            .and_then(|s| s.as_str())
            .map(str::to_string),
        mountpoints,
        fs_total_bytes,
        fs_used_bytes,
        fs_avail_bytes,
        children,
    }
}

fn parse_mountpoints(node: &Value) -> Vec<String> {
    node.get("mountpoints")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn json_size_u64(v: &Value) -> Option<u64> {
    v.as_u64()
        .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
}

fn query_statvfs(mountpoint: &str) -> Option<(u64, u64, u64)> {
    let c_path = CString::new(mountpoint).ok()?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) };
    if rc != 0 {
        return None;
    }
    let frsize = if stat.f_frsize > 0 {
        stat.f_frsize as u64
    } else {
        stat.f_bsize as u64
    };
    let total = (stat.f_blocks as u64).saturating_mul(frsize);
    let free = (stat.f_bfree as u64).saturating_mul(frsize);
    let avail = (stat.f_bavail as u64).saturating_mul(frsize);
    let used = total.saturating_sub(free);
    Some((total, used, avail))
}

fn read_sysfs_for_block(block_name: &str, mut info: SysfsInfo) -> (SysfsInfo, Vec<TempSensor>) {
    let block_dir = PathBuf::from("/sys/block").join(block_name);
    let queue_dir = block_dir.join("queue");
    let dev_dir = block_dir.join("device");
    let mut sensors = Vec::new();

    info.block_name = Some(block_name.to_string());
    if info.lsblk_size_bytes.is_none()
        && let Some(sectors) = read_u64(&block_dir.join("size"))
    {
        info.lsblk_size_bytes = Some(sectors * 512);
    }

    info.scheduler = read_trimmed(&queue_dir.join("scheduler"));
    if info.rotational.is_none() {
        info.rotational = read_u64(&queue_dir.join("rotational")).map(|v| v != 0);
    }
    info.logical_block_size = read_u64(&queue_dir.join("logical_block_size"));
    info.physical_block_size = read_u64(&queue_dir.join("physical_block_size"));
    info.hw_sector_size = read_u64(&queue_dir.join("hw_sector_size"));
    info.minimum_io_size = read_u64(&queue_dir.join("minimum_io_size"));
    info.optimal_io_size = read_u64(&queue_dir.join("optimal_io_size"));
    info.max_sectors_kb = read_u64(&queue_dir.join("max_sectors_kb"));
    info.read_ahead_kb = read_u64(&queue_dir.join("read_ahead_kb"));
    info.nr_requests = read_u64(&queue_dir.join("nr_requests"));
    info.discard_max_bytes = read_u64(&queue_dir.join("discard_max_bytes"));
    info.discard_granularity = read_u64(&queue_dir.join("discard_granularity"));
    info.write_cache = read_trimmed(&queue_dir.join("write_cache"));
    info.fua = read_u64(&queue_dir.join("fua")).map(|v| v != 0);
    info.zoned = read_trimmed(&queue_dir.join("zoned"));
    info.ncq_prio_supported = read_u64(&dev_dir.join("ncq_prio_supported")).map(|v| v != 0);
    info.queue_depth = read_u64(&dev_dir.join("queue_depth")).map(|v| v as u32);
    info.wwid =
        read_trimmed(&dev_dir.join("wwid")).or_else(|| read_trimmed(&block_dir.join("wwid")));

    if info.lsblk_model.is_none() {
        info.lsblk_model = read_trimmed(&dev_dir.join("model"));
    }
    if info.lsblk_serial.is_none() {
        info.lsblk_serial = read_trimmed(&dev_dir.join("serial"));
    }
    info.lsblk_firmware =
        read_trimmed(&dev_dir.join("firmware_rev")).or_else(|| read_trimmed(&dev_dir.join("rev")));
    info.vendor = read_trimmed(&dev_dir.join("vendor"));
    info.scsi_level = read_trimmed(&dev_dir.join("scsi_level"));

    if let Some(stat_str) = read_trimmed(&block_dir.join("stat")) {
        info.io_stats = parse_block_stat(&stat_str);
    }

    // NVMe controller sysfs
    if let Some(ctrl_name) = nvme_controller_name(block_name) {
        let nvme_ctrl = PathBuf::from("/sys/class/nvme").join(&ctrl_name);
        if nvme_ctrl.exists() {
            if info.lsblk_model.is_none() {
                info.lsblk_model = read_trimmed(&nvme_ctrl.join("model"));
            }
            if info.lsblk_serial.is_none() {
                info.lsblk_serial = read_trimmed(&nvme_ctrl.join("serial"));
            }
            if info.lsblk_firmware.is_none() {
                info.lsblk_firmware = read_trimmed(&nvme_ctrl.join("firmware_rev"));
            }
            info.nvme_transport = read_trimmed(&nvme_ctrl.join("transport"));
            info.nvme_state = read_trimmed(&nvme_ctrl.join("state"));
            info.nvme_cntlid = read_trimmed(&nvme_ctrl.join("cntlid"));
            info.nvme_subsysnqn = read_trimmed(&nvme_ctrl.join("subsysnqn"));
            info.nvme_queue_count = read_u64(&nvme_ctrl.join("queue_count")).map(|v| v as u32);
            info.nvme_sqsize = read_u64(&nvme_ctrl.join("sqsize")).map(|v| v as u32);
            info.nvme_hmb = read_trimmed(&nvme_ctrl.join("hmb"));

            let pci_dir = nvme_ctrl.join("device");
            read_pci_info(&pci_dir, &mut info);
            sensors.extend(read_hwmon_sensors(&nvme_ctrl));
        }
    } else {
        // Try reading SATA link speed if ATA device
        if let Ok(canon) = fs::canonicalize(&dev_dir)
            && let Some(sata_spd) = find_ata_link_speed(&canon)
        {
            info.sata_link_speed = Some(sata_spd);
        }
        sensors.extend(read_hwmon_sensors(&dev_dir));
    }

    (info, sensors)
}

fn read_pci_info(pci_dir: &Path, info: &mut SysfsInfo) {
    if let Ok(canon) = fs::canonicalize(pci_dir)
        && let Some(bdf) = canon.file_name().and_then(|s| s.to_str())
    {
        info.pci_bdf = Some(bdf.to_string());
    }
    info.pci_vendor_id = read_trimmed(&pci_dir.join("vendor"));
    info.pci_device_id = read_trimmed(&pci_dir.join("device"));
    info.pci_subsystem_vendor = read_trimmed(&pci_dir.join("subsystem_vendor"));
    info.pci_subsystem_device = read_trimmed(&pci_dir.join("subsystem_device"));
    info.current_link_speed = read_trimmed(&pci_dir.join("current_link_speed"));
    info.current_link_width = read_u64(&pci_dir.join("current_link_width")).map(|v| v as u32);
    info.max_link_speed = read_trimmed(&pci_dir.join("max_link_speed"));
    info.max_link_width = read_u64(&pci_dir.join("max_link_width")).map(|v| v as u32);
    info.numa_node = read_trimmed(&pci_dir.join("numa_node")).and_then(|s| s.parse().ok());
}

pub fn read_hwmon_sensors(base_dir: &Path) -> Vec<TempSensor> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(base_dir) else {
        return out;
    };
    for ent in entries.flatten() {
        let fname = ent.file_name().to_string_lossy().to_string();
        if !fname.starts_with("hwmon") {
            continue;
        }
        let path = ent.path();
        if fname == "hwmon" {
            // SCSI / drivetemp places hwmonN inside `<device>/hwmon/hwmonN/`
            if let Ok(sub_entries) = fs::read_dir(&path) {
                for sub in sub_entries.flatten() {
                    if sub.file_name().to_string_lossy().starts_with("hwmon") {
                        collect_hwmon_dir(&sub.path(), &mut out);
                    }
                }
            }
        } else {
            collect_hwmon_dir(&path, &mut out);
        }
    }
    out
}

fn collect_hwmon_dir(hwmon_dir: &Path, out: &mut Vec<TempSensor>) {
    for idx in 1..=16 {
        let input_path = hwmon_dir.join(format!("temp{}_input", idx));
        let Some(milli) = read_trimmed(&input_path).and_then(|s| s.parse::<i64>().ok()) else {
            continue;
        };
        let label = read_trimmed(&hwmon_dir.join(format!("temp{}_label", idx)))
            .unwrap_or_else(|| format!("Sensor {}", idx));
        let max_c = read_trimmed(&hwmon_dir.join(format!("temp{}_max", idx)))
            .and_then(|s| s.parse::<i64>().ok())
            .map(|m| m as f64 / 1000.0);
        let crit_c = read_trimmed(&hwmon_dir.join(format!("temp{}_crit", idx)))
            .and_then(|s| s.parse::<i64>().ok())
            .map(|m| m as f64 / 1000.0);
        out.push(TempSensor {
            label,
            celsius: milli as f64 / 1000.0,
            max_celsius: max_c,
            crit_celsius: crit_c,
        });
    }
}

pub fn find_ata_link_speed(scsi_dev_canon: &Path) -> Option<String> {
    find_ata_link_speed_with_class_dir(scsi_dev_canon, Path::new("/sys/class/ata_link"))
}

pub fn find_ata_link_speed_with_class_dir(
    scsi_dev_canon: &Path,
    ata_link_class_dir: &Path,
) -> Option<String> {
    // Inspect `<ancestor>/link*/ata_link/link*/sata_spd` first,
    // then fall back to `/sys/class/ata_link/linkN/sata_spd` based on `ataN`.
    for ancestor in scsi_dev_canon.ancestors() {
        if let Ok(entries) = fs::read_dir(ancestor) {
            let mut link_dirs: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|s| s.to_str())
                        .is_some_and(|n| n.starts_with("link"))
                })
                .collect();
            link_dirs.sort();
            for link_dir in link_dirs {
                let ata_link_sub = link_dir.join("ata_link");
                if let Ok(sub_entries) = fs::read_dir(&ata_link_sub) {
                    let mut inner_links: Vec<PathBuf> = sub_entries
                        .flatten()
                        .map(|e| e.path())
                        .filter(|p| {
                            p.file_name()
                                .and_then(|s| s.to_str())
                                .is_some_and(|n| n.starts_with("link"))
                        })
                        .collect();
                    inner_links.sort();
                    for inner in inner_links {
                        if let Some(spd) = read_valid_sata_spd(&inner.join("sata_spd")) {
                            return Some(spd);
                        }
                    }
                }
            }
        }

        let Some(name) = ancestor.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if let Some(num) = name.strip_prefix("ata")
            && !num.is_empty()
            && num.chars().all(|c| c.is_ascii_digit())
        {
            let link_path = ata_link_class_dir
                .join(format!("link{}", num))
                .join("sata_spd");
            if let Some(spd) = read_valid_sata_spd(&link_path) {
                return Some(spd);
            }
        }
    }
    None
}

fn read_valid_sata_spd(path: &Path) -> Option<String> {
    let spd = read_trimmed(path)?;
    if spd.is_empty() || spd == "<unknown>" {
        None
    } else {
        Some(spd)
    }
}

pub fn parse_block_stat(stat_str: &str) -> Option<BlockIoStats> {
    let vals: Vec<u64> = stat_str
        .split_whitespace()
        .filter_map(|s| s.parse::<u64>().ok())
        .collect();
    if vals.len() < 11 {
        return None;
    }
    Some(BlockIoStats {
        reads_completed: vals[0],
        reads_merged: vals[1],
        sectors_read: vals[2],
        read_time_ms: vals[3],
        writes_completed: vals[4],
        writes_merged: vals[5],
        sectors_written: vals[6],
        write_time_ms: vals[7],
        io_in_flight: vals[8],
        io_time_ms: vals[9],
        weighted_io_time_ms: vals[10],
        discards_completed: *vals.get(11).unwrap_or(&0),
        sectors_discarded: *vals.get(13).unwrap_or(&0),
    })
}

fn read_trimmed(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn read_u64(path: &Path) -> Option<u64> {
    read_trimmed(path)?.parse::<u64>().ok()
}
