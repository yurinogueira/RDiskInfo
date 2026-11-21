use std::fs::OpenOptions;
use std::os::fd::AsRawFd;

const SG_IO: libc::Ioctl = 0x2285;
const SG_DXFER_FROM_DEV: libc::c_int = -3;

// _IO('N', 0x40) -> returns NSID for namespace block devices
const NVME_IOCTL_ID: libc::Ioctl = 0x0000_4E40;

// _IOWR('N', 0x41, struct nvme_passthru_cmd) -> size 72 bytes
const NVME_IOCTL_ADMIN_CMD: libc::Ioctl = 0xC048_4E41;

#[repr(C)]
pub struct SgIoHdr {
    pub interface_id: libc::c_int,
    pub dxfer_direction: libc::c_int,
    pub cmd_len: libc::c_uchar,
    pub mx_sb_len: libc::c_uchar,
    pub iovec_count: libc::c_ushort,
    pub dxfer_len: libc::c_uint,
    pub dxferp: *mut libc::c_void,
    pub cmdp: *mut libc::c_uchar,
    pub sbp: *mut libc::c_uchar,
    pub timeout: libc::c_uint,
    pub flags: libc::c_uint,
    pub pack_id: libc::c_int,
    pub usr_ptr: *mut libc::c_void,
    pub status: libc::c_uchar,
    pub masked_status: libc::c_uchar,
    pub msg_status: libc::c_uchar,
    pub sb_len_wr: libc::c_uchar,
    pub host_status: libc::c_ushort,
    pub driver_status: libc::c_ushort,
    pub resid: libc::c_int,
    pub duration: libc::c_uint,
    pub info: libc::c_uint,
}

impl SgIoHdr {
    pub fn zeroed() -> Self {
        // SAFETY: SgIoHdr is a C POD struct where all-zero bits are valid.
        unsafe { std::mem::zeroed() }
    }
}

#[repr(C)]
struct NvmePassthruCmd {
    opcode: u8,
    flags: u8,
    rsvd1: u16,
    nsid: u32,
    cdw2: u32,
    cdw3: u32,
    metadata: u64,
    addr: u64,
    metadata_len: u32,
    data_len: u32,
    cdw10: u32,
    cdw11: u32,
    cdw12: u32,
    cdw13: u32,
    cdw14: u32,
    cdw15: u32,
    timeout_ms: u32,
    result: u32,
}

const _: () = assert!(std::mem::size_of::<NvmePassthruCmd>() == 72);

pub fn validate_device_path(device_path: &str) -> Result<(), String> {
    let Some(rest) = device_path.strip_prefix("/dev/") else {
        return Err("Device path must start with /dev/".to_string());
    };
    if rest.is_empty()
        || device_path.contains("..")
        || !rest
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '.'))
    {
        return Err("Invalid characters in device path".to_string());
    }
    Ok(())
}

pub fn read_raw_smart_binary(device_path: &str) -> Result<Vec<u8>, String> {
    validate_device_path(device_path)?;
    let canon = std::fs::canonicalize(device_path)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| device_path.to_string());
    if !canon.starts_with("/dev/") {
        return Err("Resolved device path must stay inside /dev/".to_string());
    }
    let short = canon.rsplit('/').next().unwrap_or(&canon);
    if short.starts_with("nvme") || short.starts_with("ng") || device_path.contains("nvme") {
        read_nvme_raw_data(device_path)
    } else {
        read_ata_scsi_raw_data(device_path)
    }
}

pub fn sg_io_succeeded(rc: libc::c_int, hdr: &SgIoHdr) -> bool {
    rc == 0
        && hdr.status == 0
        && hdr.host_status == 0
        && hdr.driver_status == 0
        && (hdr.info & 0x01) == 0
        && (hdr.dxfer_len == 0 || hdr.resid < hdr.dxfer_len as i32)
}

pub fn build_ata_passthrough_12(
    protocol_byte: u8,
    feature: u8,
    count: u8,
    lba_mid: u8,
    lba_high: u8,
    command: u8,
) -> [u8; 12] {
    [
        0xa1,
        protocol_byte,
        0x0e,
        feature,
        count,
        0,
        lba_mid,
        lba_high,
        0,
        command,
        0,
        0,
    ]
}

pub fn build_ata_passthrough_16(
    protocol_byte: u8,
    feature: u8,
    count: u8,
    lba_mid: u8,
    lba_high: u8,
    command: u8,
) -> [u8; 16] {
    [
        0x85,
        protocol_byte,
        0x0e,
        0,
        feature,
        0,
        count,
        0,
        0,
        0,
        lba_mid,
        0,
        lba_high,
        0,
        command,
        0,
    ]
}

fn run_sg_io(fd: std::os::fd::RawFd, cmd: &mut [u8], resp: &mut [u8]) -> bool {
    let mut sense_buf = [0u8; 32];
    let mut hdr = SgIoHdr::zeroed();
    hdr.interface_id = 'S' as libc::c_int;
    hdr.dxfer_direction = SG_DXFER_FROM_DEV;
    hdr.cmd_len = cmd.len() as u8;
    hdr.mx_sb_len = sense_buf.len() as u8;
    hdr.dxfer_len = resp.len() as u32;
    hdr.dxferp = resp.as_mut_ptr().cast::<libc::c_void>();
    hdr.cmdp = cmd.as_mut_ptr();
    hdr.sbp = sense_buf.as_mut_ptr();
    hdr.timeout = 20_000;

    let rc = unsafe { libc::ioctl(fd, SG_IO, &mut hdr) };
    sg_io_succeeded(rc, &hdr)
}

fn run_ata_pio_read(
    fd: std::os::fd::RawFd,
    feature: u8,
    lba_mid: u8,
    lba_high: u8,
    command: u8,
    resp: &mut [u8; 512],
) -> bool {
    // Try ATA PASS-THROUGH(12) with standard PIO Data-In (0x08)
    resp.fill(0);
    let mut cmd12 = build_ata_passthrough_12(0x08, feature, 1, lba_mid, lba_high, command);
    if run_sg_io(fd, &mut cmd12, resp) {
        return true;
    }

    // Legacy quirk retry for SMART commands on bridges expecting 0x0c
    if command == 0xb0 {
        resp.fill(0);
        let mut cmd12_compat =
            build_ata_passthrough_12(0x0c, feature, 1, lba_mid, lba_high, command);
        if run_sg_io(fd, &mut cmd12_compat, resp) {
            return true;
        }
    }

    // Fallback to ATA PASS-THROUGH(16) (0x85) for SAS HBAs / USB bridges that reject 12-byte CDBs
    resp.fill(0);
    let mut cmd16 = build_ata_passthrough_16(0x08, feature, 1, lba_mid, lba_high, command);
    run_sg_io(fd, &mut cmd16, resp)
}

fn read_ata_scsi_raw_data(device_path: &str) -> Result<Vec<u8>, String> {
    let file = OpenOptions::new()
        .read(true)
        .open(device_path)
        .map_err(|e| format!("Failed to open {}: {}", device_path, e))?;
    let fd = file.as_raw_fd();

    let mut result = Vec::new();
    let mut buf = [0u8; 512];

    // 1. SMART READ DATA (command 0xB0, feature 0xD0, LBA 0xC24F00)
    if run_ata_pio_read(fd, 0xd0, 0x4f, 0xc2, 0xb0, &mut buf) {
        result.extend_from_slice(&buf);
    }

    // 2. SMART READ THRESHOLDS (command 0xB0, feature 0xD1, LBA 0xC24F00)
    if run_ata_pio_read(fd, 0xd1, 0x4f, 0xc2, 0xb0, &mut buf) {
        result.extend_from_slice(&buf);
    }

    // 3. IDENTIFY DEVICE (command 0xEC)
    if run_ata_pio_read(fd, 0x00, 0x00, 0x00, 0xec, &mut buf) {
        result.extend_from_slice(&buf);
    }

    // 4. SCSI INQUIRY (0x12), 96 bytes
    let mut inquiry_cmd: [u8; 6] = [0x12, 0, 0, 0, 96, 0];
    let mut inquiry_resp = [0u8; 96];
    if run_sg_io(fd, &mut inquiry_cmd, &mut inquiry_resp) {
        result.extend_from_slice(&inquiry_resp);
    }

    if result.is_empty() {
        Err(format!(
            "SG_IO ioctl failed on {} (errno={})",
            device_path,
            std::io::Error::last_os_error()
        ))
    } else {
        Ok(result)
    }
}

fn read_nvme_raw_data(device_path: &str) -> Result<Vec<u8>, String> {
    let file = OpenOptions::new()
        .read(true)
        .open(device_path)
        .map_err(|e| format!("Failed to open {}: {}", device_path, e))?;
    let fd = file.as_raw_fd();

    let mut result = Vec::new();

    // 1. NVMe Get Log Page (SMART / Health Information Log, LID = 0x02, 512 bytes = 128 dwords -> NUMDL = 127)
    let mut smart_log = [0u8; 512];
    let mut smart_ok = false;
    for nsid in [0xFFFF_FFFFu32, 0, 1] {
        smart_log.fill(0);
        let mut cmd_smart: NvmePassthruCmd = unsafe { std::mem::zeroed() };
        cmd_smart.opcode = 0x02;
        cmd_smart.nsid = nsid;
        cmd_smart.addr = smart_log.as_mut_ptr() as u64;
        cmd_smart.data_len = smart_log.len() as u32;
        cmd_smart.cdw10 = (127 << 16) | 0x02;
        cmd_smart.timeout_ms = 20_000;

        let rc1 = unsafe { libc::ioctl(fd, NVME_IOCTL_ADMIN_CMD, &mut cmd_smart) };
        if rc1 == 0 {
            smart_ok = true;
            break;
        }
    }
    if smart_ok {
        result.extend_from_slice(&smart_log);
    }

    // 2. NVMe Identify Controller (opcode 0x06, CNS = 0x01, 4096 bytes)
    let mut identify_ctrl = [0u8; 4096];
    let mut cmd_id: NvmePassthruCmd = unsafe { std::mem::zeroed() };
    cmd_id.opcode = 0x06;
    cmd_id.nsid = 0;
    cmd_id.addr = identify_ctrl.as_mut_ptr() as u64;
    cmd_id.data_len = identify_ctrl.len() as u32;
    cmd_id.cdw10 = 1;
    cmd_id.timeout_ms = 20_000;

    let rc2 = unsafe { libc::ioctl(fd, NVME_IOCTL_ADMIN_CMD, &mut cmd_id) };
    let id_ctrl_ok = rc2 == 0;
    if id_ctrl_ok {
        result.extend_from_slice(&identify_ctrl);
    }

    // 3. NVMe Identify Namespace (opcode 0x06, CNS = 0x00, 4096 bytes)
    let detected_nsid = unsafe { libc::ioctl(fd, NVME_IOCTL_ID) };
    let ns_to_try = if detected_nsid > 0 {
        detected_nsid as u32
    } else {
        1
    };
    let mut identify_ns = [0u8; 4096];
    let mut cmd_ns: NvmePassthruCmd = unsafe { std::mem::zeroed() };
    cmd_ns.opcode = 0x06;
    cmd_ns.nsid = ns_to_try;
    cmd_ns.addr = identify_ns.as_mut_ptr() as u64;
    cmd_ns.data_len = identify_ns.len() as u32;
    cmd_ns.cdw10 = 0; // CNS = 0x00
    cmd_ns.timeout_ms = 20_000;
    let rc3 = unsafe { libc::ioctl(fd, NVME_IOCTL_ADMIN_CMD, &mut cmd_ns) };
    if rc3 == 0 {
        result.extend_from_slice(&identify_ns);
    }

    // 4. NVMe Error Information Log (opcode 0x02, LID = 0x01)
    let elpe_entries = if id_ctrl_ok {
        (identify_ctrl[262] as usize + 1).clamp(1, 64)
    } else {
        8
    };
    let err_len = elpe_entries * 64;
    let numdl = ((err_len / 4) - 1) as u32;
    let mut err_log = vec![0u8; err_len];
    for nsid in [0xFFFF_FFFFu32, 0] {
        err_log.fill(0);
        let mut cmd_err: NvmePassthruCmd = unsafe { std::mem::zeroed() };
        cmd_err.opcode = 0x02;
        cmd_err.nsid = nsid;
        cmd_err.addr = err_log.as_mut_ptr() as u64;
        cmd_err.data_len = err_log.len() as u32;
        cmd_err.cdw10 = (numdl << 16) | 0x01;
        cmd_err.timeout_ms = 20_000;
        let rc4 = unsafe { libc::ioctl(fd, NVME_IOCTL_ADMIN_CMD, &mut cmd_err) };
        if rc4 == 0 {
            result.extend_from_slice(&err_log);
            break;
        }
    }

    if result.is_empty() {
        Err(format!(
            "NVME_IOCTL_ADMIN_CMD failed on {} (errno={})",
            device_path,
            std::io::Error::last_os_error()
        ))
    } else {
        Ok(result)
    }
}

pub fn hex_dump(data: &[u8]) -> String {
    let mut result = String::new();
    for (chunk_idx, chunk) in data.chunks(16).enumerate() {
        let offset = chunk_idx * 16;
        result.push_str(&format!("{:08X}  ", offset));

        for j in 0..16 {
            if let Some(byte) = chunk.get(j) {
                result.push_str(&format!("{:02X} ", byte));
            } else {
                result.push_str("   ");
            }
            if j == 7 {
                result.push(' ');
            }
        }

        result.push(' ');
        for byte in chunk {
            let ch = if (32..=126).contains(byte) {
                *byte as char
            } else {
                '.'
            };
            result.push(ch);
        }
        result.push('\n');
    }
    result
}

pub fn bytes_to_hex(data: &[u8]) -> String {
    let mut s = String::with_capacity(data.len() * 2);
    for b in data {
        use std::fmt::Write;
        let _ = write!(&mut s, "{:02x}", b);
    }
    s
}

pub fn hex_to_bytes(hex: &str) -> Result<Vec<u8>, String> {
    if !hex.len().is_multiple_of(2) {
        return Err("Odd hex string length".to_string());
    }
    let mut out = Vec::with_capacity(hex.len() / 2);
    let bytes = hex.as_bytes();
    for i in (0..bytes.len()).step_by(2) {
        let s = std::str::from_utf8(&bytes[i..i + 2]).map_err(|e| e.to_string())?;
        let val = u8::from_str_radix(s, 16).map_err(|e| e.to_string())?;
        out.push(val);
    }
    Ok(out)
}
