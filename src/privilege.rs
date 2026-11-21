use crate::model::{DiskInfo, HealthEvalOptions, HealthStatus};
use crate::raw_ioctl::{bytes_to_hex, hex_to_bytes, read_raw_smart_binary, validate_device_path};
use crate::smartctl::{
    ScannedDevice, evaluate_health, merge_sysfs_and_partitions, parse_scan_json,
    parse_smartctl_device_json,
};
use crate::sysfs::{
    collect_block_devices, find_bundle_for_smart_device, is_raid_controller_dev_type,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::env;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ElevationState {
    RootNative,
    ElevatedHelper,
    UnprivilegedFullAccess,
    Denied { message: String },
    Unavailable { message: String },
    NotRequested,
}

impl ElevationState {
    pub fn summary(&self) -> String {
        match self {
            ElevationState::RootNative => "Root".to_string(),
            ElevationState::ElevatedHelper => "Elevated helper".to_string(),
            ElevationState::UnprivilegedFullAccess => "Unprivileged full access".to_string(),
            ElevationState::Denied { message } | ElevationState::Unavailable { message } => {
                message.clone()
            }
            ElevationState::NotRequested => "Not requested".to_string(),
        }
    }
}

impl std::fmt::Display for ElevationState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.summary())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanReport {
    pub disks: Vec<DiskInfo>,
    pub elevation: ElevationState,
    pub smartctl_version: Option<String>,
    pub smartctl_missing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartctlScanDeviceOutput {
    pub device_node: String,
    pub dev_type: String,
    pub json_output: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SelfTestOutcome {
    Started,
    AlreadyRunning,
    #[default]
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfTestResult {
    pub ok: bool,
    pub already_running: bool,
    #[serde(default)]
    pub outcome: SelfTestOutcome,
    pub progress_info: Option<String>,
    pub message: String,
    pub raw_json: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "cmd")]
enum HelperRequest {
    Ping,
    ScanAll,
    ReadRawBinary {
        device_path: String,
    },
    StartSelfTest {
        device_path: String,
        #[serde(default)]
        dev_type: String,
        test_type: String,
    },
    AbortSelfTest {
        device_path: String,
        #[serde(default)]
        dev_type: String,
    },
    Shutdown,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status")]
enum HelperResponse {
    Pong,
    ScanResult {
        smartctl_version: Option<String>,
        devices: Vec<SmartctlScanDeviceOutput>,
    },
    RawBinary {
        hex_data: String,
    },
    CommandOutput {
        exit_code: i32,
        stdout: String,
        stderr: String,
    },
    Error {
        message: String,
    },
}

pub fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

pub fn find_smartctl_path() -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = env::var_os("PATH")
        .map(|p| env::split_paths(&p).collect())
        .unwrap_or_default();
    for extra in ["/usr/bin", "/usr/sbin", "/usr/local/bin", "/usr/local/sbin"] {
        let p = PathBuf::from(extra);
        if !dirs.contains(&p) {
            dirs.push(p);
        }
    }
    for dir in dirs {
        let candidate = dir.join("smartctl");
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn is_executable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    meta.is_file() && (meta.permissions().mode() & 0o111 != 0)
}

pub fn find_pkexec_path() -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = env::var_os("PATH")
        .map(|p| env::split_paths(&p).collect())
        .unwrap_or_default();
    for extra in ["/usr/bin", "/bin"] {
        let p = PathBuf::from(extra);
        if !dirs.contains(&p) {
            dirs.push(p);
        }
    }
    for dir in dirs {
        let candidate = dir.join("pkexec");
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

pub fn can_prompt_elevation() -> bool {
    let has_display = env::var_os("DISPLAY").is_some_and(|v| !v.is_empty())
        || env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty());
    let has_tty = unsafe { libc::isatty(libc::STDIN_FILENO) == 1 };
    has_display || has_tty
}

pub struct HelperClient {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    alive: bool,
}

impl HelperClient {
    pub fn spawn() -> Result<Self, ElevationState> {
        let Some(pkexec) = find_pkexec_path() else {
            return Err(ElevationState::Unavailable {
                message: "pkexec not found".to_string(),
            });
        };
        if !can_prompt_elevation() {
            return Err(ElevationState::Unavailable {
                message: "No graphical display or TTY available for pkexec authentication"
                    .to_string(),
            });
        }
        let exe = env::current_exe().map_err(|e| ElevationState::Unavailable {
            message: format!("Failed to locate executable: {}", e),
        })?;

        let mut child = Command::new(pkexec)
            .arg(exe)
            .arg("--privileged-helper")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| ElevationState::Unavailable {
                message: format!("Failed to start pkexec: {}", e),
            })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| ElevationState::Unavailable {
                message: "Missing stdin pipe".to_string(),
            })?;
        let stdout_raw = child
            .stdout
            .take()
            .ok_or_else(|| ElevationState::Unavailable {
                message: "Missing stdout pipe".to_string(),
            })?;

        let mut client = Self {
            child,
            stdin: Some(stdin),
            stdout: BufReader::new(stdout_raw),
            alive: true,
        };

        // Ping verifies polkit auth succeeded and the root helper is ready
        match client.request(&HelperRequest::Ping) {
            Ok(HelperResponse::Pong) => Ok(client),
            Ok(HelperResponse::Error { message }) => {
                client.alive = false;
                Err(ElevationState::Denied { message })
            }
            Ok(_) => {
                client.alive = false;
                Err(ElevationState::Unavailable {
                    message: "Unexpected helper handshake".to_string(),
                })
            }
            Err(err_msg) => {
                client.alive = false;
                // Close stdin first so pkexec won't block waiting for input
                client.stdin.take();
                let mut status_code = None;
                for _ in 0..10 {
                    if let Ok(Some(st)) = client.child.try_wait() {
                        status_code = st.code();
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
                if status_code.is_none() {
                    let _ = client.child.kill();
                    status_code = client.child.wait().ok().and_then(|s| s.code());
                }
                let mut stderr_msg = String::new();
                if let Some(mut err_pipe) = client.child.stderr.take() {
                    use std::io::Read;
                    let _ = err_pipe.read_to_string(&mut stderr_msg);
                }
                let stderr_clean = stderr_msg.trim();
                if matches!(status_code, Some(126) | Some(127)) {
                    let msg = if stderr_clean.is_empty() {
                        "Authentication cancelled or denied".to_string()
                    } else {
                        stderr_clean.to_string()
                    };
                    Err(ElevationState::Denied { message: msg })
                } else {
                    let msg = if stderr_clean.is_empty() {
                        err_msg
                    } else {
                        format!("{} ({})", err_msg, stderr_clean)
                    };
                    Err(ElevationState::Denied { message: msg })
                }
            }
        }
    }

    fn request(&mut self, req: &HelperRequest) -> Result<HelperResponse, String> {
        let Some(stdin) = self.stdin.as_mut() else {
            return Err("Privileged helper stdin closed".to_string());
        };
        let mut line = serde_json::to_string(req).map_err(|e| e.to_string())?;
        line.push('\n');
        stdin
            .write_all(line.as_bytes())
            .map_err(|e| format!("Helper write error: {}", e))?;
        stdin
            .flush()
            .map_err(|e| format!("Helper flush error: {}", e))?;

        let mut resp_line = String::new();
        let n = self
            .stdout
            .read_line(&mut resp_line)
            .map_err(|e| format!("Helper read error: {}", e))?;
        if n == 0 {
            return Err("Privileged helper closed connection".to_string());
        }
        serde_json::from_str::<HelperResponse>(&resp_line)
            .map_err(|e| format!("Invalid helper JSON: {}", e))
    }
}

impl Drop for HelperClient {
    fn drop(&mut self) {
        if self.alive
            && let Some(mut stdin) = self.stdin.take()
            && let Ok(mut msg) = serde_json::to_string(&HelperRequest::Shutdown)
        {
            msg.push('\n');
            let _ = stdin.write_all(msg.as_bytes());
            let _ = stdin.flush();
        } else {
            self.stdin.take();
        }
        for _ in 0..8 {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Clone, Default)]
pub struct BackendManager {
    helper: Arc<Mutex<Option<HelperClient>>>,
}

impl BackendManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn has_active_helper(&self) -> bool {
        self.helper.lock().map(|g| g.is_some()).unwrap_or(false)
    }

    pub fn ensure_elevated(&self) -> Result<(), ElevationState> {
        if is_root() {
            return Ok(());
        }
        let mut guard = self
            .helper
            .lock()
            .map_err(|_| ElevationState::Unavailable {
                message: "Helper mutex poisoned".to_string(),
            })?;
        if let Some(client) = guard.as_mut() {
            if matches!(
                client.request(&HelperRequest::Ping),
                Ok(HelperResponse::Pong)
            ) {
                return Ok(());
            }
            *guard = None;
        }
        let client = HelperClient::spawn()?;
        *guard = Some(client);
        Ok(())
    }

    pub fn scan_unprivileged_fast(&self, eval_opts: &HealthEvalOptions) -> ScanReport {
        let bundles = collect_block_devices();
        let smartctl_path = find_smartctl_path();
        let smartctl_version = smartctl_path.as_deref().and_then(get_smartctl_version);
        let disks = build_unprivileged_disks(&bundles, eval_opts);
        ScanReport {
            disks,
            elevation: if is_root() {
                ElevationState::RootNative
            } else {
                ElevationState::NotRequested
            },
            smartctl_version,
            smartctl_missing: smartctl_path.is_none(),
        }
    }

    pub fn scan_disks(&self, auto_elevate: bool, eval_opts: &HealthEvalOptions) -> ScanReport {
        let bundles = collect_block_devices();
        let smartctl_path = find_smartctl_path();
        let Some(smartctl_bin) = smartctl_path else {
            let disks = build_unprivileged_disks(&bundles, eval_opts);
            return ScanReport {
                disks,
                elevation: ElevationState::Unavailable {
                    message: "smartctl not found".to_string(),
                },
                smartctl_version: None,
                smartctl_missing: true,
            };
        };

        // Check if we're root or can read all candidate nodes without root
        let unpriv_scan = run_smartctl_scan(&smartctl_bin);
        let bundle_nodes: Vec<String> = bundles
            .values()
            .map(|b| b.smart_device_node.clone())
            .collect();
        let mut candidate_nodes: Vec<String> = unpriv_scan.iter().map(|d| d.name.clone()).collect();
        for node in &bundle_nodes {
            if !candidate_nodes.contains(node) {
                candidate_nodes.push(node.clone());
            }
        }

        let can_read_all_unpriv = !candidate_nodes.is_empty()
            && candidate_nodes
                .iter()
                .all(|node| OpenOptions::new().read(true).open(node).is_ok());

        let (elevation, smartctl_version, raw_outputs) = if is_root() {
            let (ver, outs) = run_smartctl_scan_all_direct(&smartctl_bin, &bundle_nodes);
            (ElevationState::RootNative, ver, outs)
        } else if can_read_all_unpriv {
            let (ver, outs) = run_smartctl_scan_all_direct(&smartctl_bin, &bundle_nodes);
            (ElevationState::UnprivilegedFullAccess, ver, outs)
        } else if auto_elevate || self.has_active_helper() {
            match self.ensure_elevated() {
                Ok(()) => {
                    if let Ok(mut guard) = self.helper.lock() {
                        if let Some(client) = guard.as_mut() {
                            match client.request(&HelperRequest::ScanAll) {
                                Ok(HelperResponse::ScanResult {
                                    smartctl_version,
                                    devices,
                                }) => (ElevationState::ElevatedHelper, smartctl_version, devices),
                                Ok(HelperResponse::Error { message }) => {
                                    *guard = None;
                                    (
                                        ElevationState::Unavailable { message },
                                        get_smartctl_version(&smartctl_bin),
                                        Vec::new(),
                                    )
                                }
                                Err(err) => {
                                    *guard = None;
                                    (
                                        ElevationState::Unavailable { message: err },
                                        get_smartctl_version(&smartctl_bin),
                                        Vec::new(),
                                    )
                                }
                                _ => (
                                    ElevationState::Unavailable {
                                        message: "Unexpected response".to_string(),
                                    },
                                    None,
                                    Vec::new(),
                                ),
                            }
                        } else {
                            (ElevationState::NotRequested, None, Vec::new())
                        }
                    } else {
                        (
                            ElevationState::Unavailable {
                                message: "Helper mutex poisoned".to_string(),
                            },
                            get_smartctl_version(&smartctl_bin),
                            Vec::new(),
                        )
                    }
                }
                Err(elev_err) => (elev_err, get_smartctl_version(&smartctl_bin), Vec::new()),
            }
        } else {
            (
                ElevationState::NotRequested,
                get_smartctl_version(&smartctl_bin),
                Vec::new(),
            )
        };

        let mut disks = Vec::new();
        let mut covered_nodes = HashSet::new();

        for item in raw_outputs {
            if let Some(mut disk) = parse_smartctl_device_json(
                &item.json_output,
                Some(&item.device_node),
                Some(&item.dev_type),
                eval_opts,
            ) {
                if !is_raid_controller_dev_type(&disk.device_type)
                    && !disk.device_node.starts_with("/dev/bus/")
                {
                    covered_nodes.insert(disk.device_node.clone());
                }
                if let Some(bundle) =
                    find_bundle_for_smart_device(&bundles, &disk.device_node, &disk.device_type)
                {
                    covered_nodes.insert(bundle.smart_device_node.clone());
                    merge_sysfs_and_partitions(
                        &mut disk,
                        bundle.sysfs.clone(),
                        bundle.partitions.clone(),
                        bundle.hwmon_sensors.clone(),
                    );
                    evaluate_health(&mut disk, eval_opts);
                } else {
                    if disk.logical_block_size == 0 {
                        disk.logical_block_size = 512;
                    }
                    if disk.physical_block_size == 0 {
                        disk.physical_block_size = disk.logical_block_size;
                    }
                }
                disks.push(disk);
            }
        }

        // Include any physical disks from sysfs/lsblk that didn't get smartctl output
        for bundle in bundles.values() {
            if !covered_nodes.contains(&bundle.smart_device_node) {
                let mut disk = DiskInfo {
                    device_node: bundle.smart_device_node.clone(),
                    health_status: HealthStatus::Unknown,
                    smartctl_available_data: false,
                    ..Default::default()
                };
                merge_sysfs_and_partitions(
                    &mut disk,
                    bundle.sysfs.clone(),
                    bundle.partitions.clone(),
                    bundle.hwmon_sensors.clone(),
                );
                evaluate_health(&mut disk, eval_opts);
                disks.push(disk);
            }
        }

        disks.sort_by(|a, b| {
            a.device_node
                .cmp(&b.device_node)
                .then_with(|| a.device_type.cmp(&b.device_type))
        });

        ScanReport {
            disks,
            elevation,
            smartctl_version,
            smartctl_missing: false,
        }
    }

    pub fn read_raw_binary(&self, device_path: &str) -> Result<Vec<u8>, String> {
        validate_device_path(device_path)?;
        if is_root() || OpenOptions::new().read(true).open(device_path).is_ok() {
            return read_raw_smart_binary(device_path);
        }
        self.ensure_elevated()
            .map_err(|e| format!("Elevation required: {}", e.summary()))?;
        let mut guard = self.helper.lock().map_err(|e| e.to_string())?;
        let client = guard
            .as_mut()
            .ok_or_else(|| "Privileged helper unavailable".to_string())?;
        match client.request(&HelperRequest::ReadRawBinary {
            device_path: device_path.to_string(),
        }) {
            Ok(HelperResponse::RawBinary { hex_data }) => hex_to_bytes(&hex_data),
            Ok(HelperResponse::Error { message }) => Err(message),
            Ok(_) => Err("Unexpected helper response".to_string()),
            Err(e) => {
                *guard = None;
                Err(e)
            }
        }
    }

    pub fn start_self_test(
        &self,
        device_path: &str,
        dev_type: &str,
        test_type: &str,
    ) -> Result<SelfTestResult, String> {
        let args = build_start_self_test_args(device_path, dev_type, test_type)?;

        let (exit_code, stdout, stderr) = if is_root() {
            let smartctl = find_smartctl_path().ok_or("smartctl not found")?;
            let out = Command::new(smartctl)
                .args(&args)
                .output()
                .map_err(|e| e.to_string())?;
            (
                out.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&out.stdout).to_string(),
                String::from_utf8_lossy(&out.stderr).to_string(),
            )
        } else {
            self.ensure_elevated()
                .map_err(|e| format!("Elevation required: {}", e.summary()))?;
            let mut guard = self.helper.lock().map_err(|e| e.to_string())?;
            let client = guard
                .as_mut()
                .ok_or_else(|| "Privileged helper unavailable".to_string())?;
            match client.request(&HelperRequest::StartSelfTest {
                device_path: device_path.to_string(),
                dev_type: dev_type.to_string(),
                test_type: test_type.to_string(),
            }) {
                Ok(HelperResponse::CommandOutput {
                    exit_code,
                    stdout,
                    stderr,
                }) => (exit_code, stdout, stderr),
                Ok(HelperResponse::Error { message }) => return Err(message),
                Ok(_) => return Err("Unexpected response".to_string()),
                Err(e) => {
                    *guard = None;
                    return Err(e);
                }
            }
        };

        Ok(parse_self_test_response(exit_code, &stdout, &stderr))
    }

    pub fn abort_self_test(&self, device_path: &str, dev_type: &str) -> Result<String, String> {
        let args = build_abort_self_test_args(device_path, dev_type)?;
        let (exit_code, stdout, stderr) = if is_root() {
            let smartctl = find_smartctl_path().ok_or("smartctl not found")?;
            let out = Command::new(smartctl)
                .args(&args)
                .output()
                .map_err(|e| e.to_string())?;
            (
                out.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&out.stdout).to_string(),
                String::from_utf8_lossy(&out.stderr).to_string(),
            )
        } else {
            self.ensure_elevated()
                .map_err(|e| format!("Elevation required: {}", e.summary()))?;
            let mut guard = self.helper.lock().map_err(|e| e.to_string())?;
            let client = guard
                .as_mut()
                .ok_or_else(|| "Privileged helper unavailable".to_string())?;
            match client.request(&HelperRequest::AbortSelfTest {
                device_path: device_path.to_string(),
                dev_type: dev_type.to_string(),
            }) {
                Ok(HelperResponse::CommandOutput {
                    exit_code,
                    stdout,
                    stderr,
                }) => (exit_code, stdout, stderr),
                Ok(HelperResponse::Error { message }) => return Err(message),
                Ok(_) => return Err("Unexpected response".to_string()),
                Err(e) => {
                    *guard = None;
                    return Err(e);
                }
            }
        };

        if exit_code >= 0 && (exit_code & 0x07) == 0 {
            Ok("Self-test aborted successfully".to_string())
        } else {
            Err(format!(
                "Abort failed (code {}): {} {}",
                exit_code,
                stdout.trim(),
                stderr.trim()
            ))
        }
    }
}

pub fn validate_dev_type(dev_type: &str) -> Result<(), String> {
    if dev_type.is_empty() {
        return Ok(());
    }
    if dev_type.len() > 64 {
        return Err("Device type is too long".to_string());
    }
    if dev_type.contains("..") {
        return Err("Invalid sequence in device type".to_string());
    }
    let Some(first) = dev_type.chars().next() else {
        return Ok(());
    };
    if !first.is_ascii_alphanumeric() {
        return Err("Device type must start with an alphanumeric character".to_string());
    }
    if !dev_type
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, ',' | '/' | '_' | '-' | '+'))
    {
        return Err("Invalid characters in device type".to_string());
    }
    Ok(())
}

pub fn build_start_self_test_args(
    device_path: &str,
    dev_type: &str,
    test_type: &str,
) -> Result<Vec<String>, String> {
    validate_device_path(device_path)?;
    validate_dev_type(dev_type)?;
    validate_test_type(test_type)?;
    let mut args = vec!["--json=o".to_string()];
    if !dev_type.is_empty() && dev_type != "scsi" {
        args.push("-d".to_string());
        args.push(dev_type.to_string());
    }
    args.push("-t".to_string());
    args.push(test_type.to_string());
    args.push(device_path.to_string());
    Ok(args)
}

pub fn build_abort_self_test_args(
    device_path: &str,
    dev_type: &str,
) -> Result<Vec<String>, String> {
    validate_device_path(device_path)?;
    validate_dev_type(dev_type)?;
    let mut args = Vec::new();
    if !dev_type.is_empty() && dev_type != "scsi" {
        args.push("-d".to_string());
        args.push(dev_type.to_string());
    }
    args.push("-X".to_string());
    args.push(device_path.to_string());
    Ok(args)
}

fn validate_test_type(test_type: &str) -> Result<(), String> {
    match test_type {
        "short" | "long" | "conveyance" => Ok(()),
        _ => Err("Invalid self-test type".to_string()),
    }
}

pub fn parse_self_test_response(exit_code: i32, stdout: &str, stderr: &str) -> SelfTestResult {
    let val = serde_json::from_str::<serde_json::Value>(stdout).ok();
    let smartctl_exit = val
        .as_ref()
        .and_then(|v| v.get("smartctl"))
        .and_then(|s| s.get("exit_status"))
        .and_then(|e| e.as_i64())
        .unwrap_or(exit_code as i64);

    let output_lines: Vec<String> = val
        .as_ref()
        .and_then(|v| v.get("smartctl"))
        .and_then(|s| s.get("output"))
        .and_then(|o| o.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    let progress_info = output_lines
        .iter()
        .find(|line| {
            line.contains('%') && (line.contains("remaining") || line.contains("completed"))
        })
        .map(|s| s.trim().to_string());

    let output_lower = output_lines.join("\n").to_ascii_lowercase();
    let started_text = output_lower.contains("testing has begun")
        || output_lower.contains("self-test has begun")
        || output_lower.contains("please wait");
    let unsupported_text = output_lower.contains("not supported")
        || output_lower.contains("unsupported")
        || output_lower.contains("invalid field")
        || output_lower.contains("unknown command");
    let already_running_text = progress_info.is_some()
        || output_lower.contains("already in progress")
        || output_lower.contains("cannot start self-test")
        || output_lower.contains("can't start self-test");

    if already_running_text || ((smartctl_exit & 0x04) != 0 && !started_text && !unsupported_text) {
        SelfTestResult {
            ok: false,
            already_running: already_running_text || (smartctl_exit & 0x04) != 0,
            outcome: SelfTestOutcome::AlreadyRunning,
            progress_info,
            message: "A self-test is already running on this drive.".to_string(),
            raw_json: stdout.to_string(),
        }
    } else if (smartctl_exit & 0x07) == 0 || started_text || exit_code == 0 {
        SelfTestResult {
            ok: true,
            already_running: false,
            outcome: SelfTestOutcome::Started,
            progress_info: None,
            message: "Self-test started successfully.".to_string(),
            raw_json: stdout.to_string(),
        }
    } else {
        SelfTestResult {
            ok: false,
            already_running: false,
            outcome: SelfTestOutcome::Failed,
            progress_info: None,
            message: if stderr.trim().is_empty() {
                format!("smartctl exited with status {}", smartctl_exit)
            } else {
                stderr.trim().to_string()
            },
            raw_json: stdout.to_string(),
        }
    }
}

fn build_unprivileged_disks(
    bundles: &std::collections::HashMap<String, crate::sysfs::BlockDeviceBundle>,
    eval_opts: &HealthEvalOptions,
) -> Vec<DiskInfo> {
    let mut disks: Vec<DiskInfo> = bundles
        .values()
        .map(|bundle| {
            let mut disk = DiskInfo {
                device_node: bundle.smart_device_node.clone(),
                health_status: HealthStatus::Unknown,
                ..Default::default()
            };
            merge_sysfs_and_partitions(
                &mut disk,
                bundle.sysfs.clone(),
                bundle.partitions.clone(),
                bundle.hwmon_sensors.clone(),
            );
            evaluate_health(&mut disk, eval_opts);
            disk
        })
        .collect();
    disks.sort_by(|a, b| {
        a.device_node
            .cmp(&b.device_node)
            .then_with(|| a.device_type.cmp(&b.device_type))
    });
    disks
}

fn run_smartctl_scan(smartctl_bin: &Path) -> Vec<ScannedDevice> {
    let Ok(out) = Command::new(smartctl_bin)
        .args(["--scan", "--json"])
        .output()
    else {
        return Vec::new();
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    parse_scan_json(&stdout)
}

fn get_smartctl_version(smartctl_bin: &Path) -> Option<String> {
    let out = Command::new(smartctl_bin).arg("--version").output().ok()?;
    let txt = String::from_utf8_lossy(&out.stdout);
    txt.lines().next().map(|l| l.trim().to_string())
}

fn normalize_scan_node(node: &str) -> String {
    let short = node.rsplit('/').next().unwrap_or(node);
    if let Some(ctrl) = crate::sysfs::nvme_controller_name(short) {
        return format!("/dev/{}", ctrl);
    }
    node.to_string()
}

pub fn collect_scan_targets(
    scanned: Vec<ScannedDevice>,
    extra_nodes: &[String],
) -> Vec<(String, String)> {
    let mut seen_pairs = HashSet::new();
    let mut seen_nodes = HashSet::new();
    let mut targets: Vec<(String, String)> = Vec::new();

    for d in scanned {
        let pair = (d.name.clone(), d.dev_type.clone());
        if seen_pairs.insert(pair) {
            if !is_raid_controller_dev_type(&d.dev_type) && !d.name.starts_with("/dev/bus/") {
                seen_nodes.insert(normalize_scan_node(&d.name));
                seen_nodes.insert(d.name.clone());
            }
            targets.push((d.name, d.dev_type));
        }
    }
    for node in extra_nodes {
        if node.starts_with("/dev/bus/") {
            continue;
        }
        let norm = normalize_scan_node(node);
        if !seen_nodes.contains(node) && !seen_nodes.contains(&norm) {
            seen_nodes.insert(norm);
            seen_nodes.insert(node.clone());
            targets.push((node.clone(), String::new()));
        }
    }
    targets
}

fn run_smartctl_scan_all_direct(
    smartctl_bin: &Path,
    extra_nodes: &[String],
) -> (Option<String>, Vec<SmartctlScanDeviceOutput>) {
    let version = get_smartctl_version(smartctl_bin);
    let scanned = run_smartctl_scan(smartctl_bin);
    let targets = collect_scan_targets(scanned, extra_nodes);

    let mut outputs = Vec::new();
    for (node, dev_type) in targets {
        if validate_device_path(&node).is_err() {
            continue;
        }
        let mut cmd = Command::new(smartctl_bin);
        cmd.arg("-x").arg("--json=o");
        if !dev_type.is_empty() && dev_type != "scsi" {
            // Let smartctl probe SATA behind scsi automatically unless explicit non-scsi type
            cmd.arg("-d").arg(&dev_type);
        }
        cmd.arg(&node);

        if let Ok(out) = cmd.output() {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            outputs.push(SmartctlScanDeviceOutput {
                device_node: node,
                dev_type,
                json_output: stdout,
                exit_code: out.status.code().unwrap_or(-1),
            });
        }
    }

    (version, outputs)
}

/// Entry point when invoked with `--privileged-helper`.
/// Reads newline-delimited JSON commands from stdin and writes JSON responses to stdout.
pub fn run_privileged_helper() -> i32 {
    if !is_root() {
        let resp = HelperResponse::Error {
            message: "Helper must run with root privileges".to_string(),
        };
        if let Ok(s) = serde_json::to_string(&resp) {
            println!("{}", s);
        }
        return 1;
    }

    let Some(smartctl_bin) = find_smartctl_path() else {
        let resp = HelperResponse::Error {
            message: "smartctl binary not found".to_string(),
        };
        if let Ok(s) = serde_json::to_string(&resp) {
            println!("{}", s);
        }
        return 1;
    };

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    for line_res in stdin.lock().lines() {
        let Ok(line) = line_res else {
            break;
        };
        if line.trim().is_empty() {
            continue;
        }
        let req: HelperRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let _ = send_helper_resp(
                    &mut stdout,
                    &HelperResponse::Error {
                        message: format!("Invalid request: {}", e),
                    },
                );
                continue;
            }
        };

        let resp = match req {
            HelperRequest::Ping => HelperResponse::Pong,
            HelperRequest::Shutdown => {
                let _ = send_helper_resp(&mut stdout, &HelperResponse::Pong);
                break;
            }
            HelperRequest::ScanAll => {
                let bundles = collect_block_devices();
                let extra: Vec<String> = bundles
                    .values()
                    .map(|b| b.smart_device_node.clone())
                    .collect();
                let (ver, devices) = run_smartctl_scan_all_direct(&smartctl_bin, &extra);
                HelperResponse::ScanResult {
                    smartctl_version: ver,
                    devices,
                }
            }
            HelperRequest::ReadRawBinary { device_path } => {
                match read_raw_smart_binary(&device_path) {
                    Ok(bytes) => HelperResponse::RawBinary {
                        hex_data: bytes_to_hex(&bytes),
                    },
                    Err(e) => HelperResponse::Error { message: e },
                }
            }
            HelperRequest::StartSelfTest {
                device_path,
                dev_type,
                test_type,
            } => match build_start_self_test_args(&device_path, &dev_type, &test_type) {
                Err(e) => HelperResponse::Error { message: e },
                Ok(args) => match Command::new(&smartctl_bin).args(&args).output() {
                    Ok(out) => HelperResponse::CommandOutput {
                        exit_code: out.status.code().unwrap_or(-1),
                        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
                        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
                    },
                    Err(e) => HelperResponse::Error {
                        message: e.to_string(),
                    },
                },
            },
            HelperRequest::AbortSelfTest {
                device_path,
                dev_type,
            } => match build_abort_self_test_args(&device_path, &dev_type) {
                Err(e) => HelperResponse::Error { message: e },
                Ok(args) => match Command::new(&smartctl_bin).args(&args).output() {
                    Ok(out) => HelperResponse::CommandOutput {
                        exit_code: out.status.code().unwrap_or(-1),
                        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
                        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
                    },
                    Err(e) => HelperResponse::Error {
                        message: e.to_string(),
                    },
                },
            },
        };

        if send_helper_resp(&mut stdout, &resp).is_err() {
            break;
        }
    }

    0
}

fn send_helper_resp(stdout: &mut std::io::Stdout, resp: &HelperResponse) -> std::io::Result<()> {
    let mut s = serde_json::to_string(resp).unwrap_or_else(|e| {
        format!(
            "{{\"status\":\"Error\",\"message\":\"{}\"}}",
            e.to_string().replace('"', "'")
        )
    });
    s.push('\n');
    stdout.write_all(s.as_bytes())?;
    stdout.flush()
}
