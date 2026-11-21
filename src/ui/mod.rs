pub mod artwork;
pub mod details_tabs;
pub mod dialogs;
pub mod disk_bar;
pub mod smart_table;
pub mod summary_panel;

use crate::config::{AppConfig, ThemeKind};
use crate::export::{
    export_disk_json, format_full_report, format_single_disk_report, save_screenshot_png,
};
use crate::i18n::{LanguageChoice, Strings};
use crate::model::{DiskInfo, DiskProtocol, TempSample, format_capacity};
use crate::privilege::{BackendManager, ElevationState, ScanReport, SelfTestResult};
use crate::raw_ioctl::hex_dump;
use crate::smartctl::evaluate_health;
use crate::theme::ThemePalette;
use crate::ui::details_tabs::SelfTestActionRequest;
use crate::ui::dialogs::{DialogAction, DialogStates, SelfTestModalState};
use eframe::egui;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DetailTab {
    Smart,
    Hardware,
    Partitions,
    Diagnostics,
    Raw,
}

enum WorkerEvent {
    ScanCompleted(ScanReport),
    AsciiCompleted {
        device_node: String,
        result: Result<Vec<u8>, String>,
    },
    SelfTestCompleted {
        device_node: String,
        device_type: String,
        result: Result<SelfTestResult, String>,
    },
    SelfTestAborted(Result<String, String>),
    Toast(String),
}

pub struct RDiskInfoApp {
    config: AppConfig,
    backend: BackendManager,
    disks: Vec<DiskInfo>,
    selected_idx: usize,
    active_tab: DetailTab,
    serial_revealed: bool,
    smart_filter: String,
    raw_show_json: bool,
    raw_filter: String,
    scanning: bool,
    elevation: ElevationState,
    smartctl_version: Option<String>,
    smartctl_missing: bool,
    dialogs: DialogStates,
    toast_msg: Option<(String, Instant)>,
    custom_bg_texture: Option<egui::TextureHandle>,
    last_refresh: Instant,
    auto_elevate_on_start: bool,
    elevation_declined: bool,
    applied_theme_state: Option<(ThemeKind, u32)>,
    pending_screenshot: Option<(PathBuf, u8)>,
    temp_history: HashMap<(String, String), Vec<TempSample>>,
    tx: Sender<WorkerEvent>,
    rx: Receiver<WorkerEvent>,
}

impl RDiskInfoApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        mut config: AppConfig,
        auto_elevate_on_start: bool,
        theme_override: Option<ThemeKind>,
    ) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        if let Some(t) = theme_override {
            config.theme = t;
        }

        let (tx, rx) = mpsc::channel();
        let backend = BackendManager::new();

        let custom_bg_texture = config
            .custom_background_path
            .as_deref()
            .and_then(|p| load_texture_from_path(&cc.egui_ctx, Path::new(p)));

        let palette = ThemePalette::from_kind(config.theme, config.panel_opacity);
        palette.apply_to_ctx(&cc.egui_ctx);
        let initial_theme = config.theme;
        let opacity_key = (config.panel_opacity * 1000.0).round() as u32;

        // Immediately show sysfs/lsblk/hwmon devices (<20ms) so UI is responsive while pkexec runs
        let initial_fast = backend.scan_unprivileged_fast(&config.eval_options());
        let mut temp_history = HashMap::new();
        record_temperature_samples(&mut temp_history, &initial_fast.disks, &current_hms_label());

        let mut app = Self {
            config,
            backend,
            disks: initial_fast.disks,
            selected_idx: 0,
            active_tab: DetailTab::Smart,
            serial_revealed: false,
            smart_filter: String::new(),
            raw_show_json: false,
            raw_filter: String::new(),
            scanning: false,
            elevation: initial_fast.elevation,
            smartctl_version: initial_fast.smartctl_version,
            smartctl_missing: initial_fast.smartctl_missing,
            dialogs: DialogStates::default(),
            toast_msg: None,
            custom_bg_texture,
            last_refresh: Instant::now(),
            auto_elevate_on_start,
            elevation_declined: false,
            applied_theme_state: Some((initial_theme, opacity_key)),
            pending_screenshot: None,
            temp_history,
            tx,
            rx,
        };

        app.trigger_scan(&cc.egui_ctx, auto_elevate_on_start);
        app
    }

    fn should_auto_elevate(&self) -> bool {
        self.auto_elevate_on_start && !self.elevation_declined
    }

    fn trigger_scan(&mut self, ctx: &egui::Context, allow_elevate: bool) {
        if self.scanning {
            return;
        }
        if allow_elevate {
            self.elevation_declined = false;
        }
        self.scanning = true;
        self.last_refresh = Instant::now();
        let backend = self.backend.clone();
        let eval_opts = self.config.eval_options();
        let tx = self.tx.clone();
        let ctx_clone = ctx.clone();

        std::thread::spawn(move || {
            let report = backend.scan_disks(allow_elevate, &eval_opts);
            let _ = tx.send(WorkerEvent::ScanCompleted(report));
            ctx_clone.request_repaint();
        });
    }

    fn trigger_ascii_view(&mut self, ctx: &egui::Context, device_node: String) {
        self.dialogs.ascii_open = true;
        self.dialogs.ascii_loading = true;
        self.dialogs.ascii_device = device_node.clone();
        self.dialogs.ascii_hex_text.clear();
        self.dialogs.ascii_raw_bytes.clear();
        self.dialogs.ascii_error = None;

        let backend = self.backend.clone();
        let tx = self.tx.clone();
        let ctx_clone = ctx.clone();

        std::thread::spawn(move || {
            let result = backend.read_raw_binary(&device_node);
            let _ = tx.send(WorkerEvent::AsciiCompleted {
                device_node,
                result,
            });
            ctx_clone.request_repaint();
        });
    }

    fn trigger_self_test(
        &mut self,
        ctx: &egui::Context,
        device_node: String,
        device_type: String,
        mode: &str,
    ) {
        let backend = self.backend.clone();
        let tx = self.tx.clone();
        let ctx_clone = ctx.clone();
        let mode_str = mode.to_string();

        std::thread::spawn(move || {
            let result = backend.start_self_test(&device_node, &device_type, &mode_str);
            let _ = tx.send(WorkerEvent::SelfTestCompleted {
                device_node,
                device_type,
                result,
            });
            ctx_clone.request_repaint();
        });
    }

    fn trigger_abort_self_test(
        &mut self,
        ctx: &egui::Context,
        device_node: String,
        device_type: String,
    ) {
        let backend = self.backend.clone();
        let tx = self.tx.clone();
        let ctx_clone = ctx.clone();

        std::thread::spawn(move || {
            let result = backend.abort_self_test(&device_node, &device_type);
            let _ = tx.send(WorkerEvent::SelfTestAborted(result));
            ctx_clone.request_repaint();
        });
    }

    fn reevaluate_all_disks(&mut self) {
        let opts = self.config.eval_options();
        for d in &mut self.disks {
            evaluate_health(d, &opts);
        }
    }

    fn set_toast(&mut self, msg: String) {
        self.toast_msg = Some((msg, Instant::now()));
    }

    fn save_selected_disk_json(&mut self, tr: &Strings) {
        if let Some(disk) = self.disks.get(self.selected_idx) {
            let stem = disk_filename_stem(&self.disks, disk);
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("JSON", &["json"])
                .set_file_name(format!("RDiskInfo_{}.json", stem))
                .save_file()
            {
                match export_disk_json(disk, &path) {
                    Ok(()) => self.set_toast(format!("{}: {}", tr.toast_saved, path.display())),
                    Err(e) => self.set_toast(format!("{}: {}", tr.toast_error, e)),
                }
            }
        }
    }

    fn poll_events(&mut self, ctx: &egui::Context, tr: &Strings) {
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                WorkerEvent::ScanCompleted(report) => {
                    let prev_sel = self
                        .disks
                        .get(self.selected_idx)
                        .map(|d| (d.device_node.clone(), d.device_type.clone()));
                    if matches!(
                        report.elevation,
                        ElevationState::Denied { .. } | ElevationState::Unavailable { .. }
                    ) {
                        self.elevation_declined = true;
                    } else if matches!(
                        report.elevation,
                        ElevationState::ElevatedHelper
                            | ElevationState::RootNative
                            | ElevationState::UnprivilegedFullAccess
                    ) {
                        self.elevation_declined = false;
                    }
                    self.disks = report.disks;
                    self.elevation = report.elevation;
                    self.smartctl_version = report.smartctl_version;
                    self.smartctl_missing = report.smartctl_missing;
                    self.scanning = false;

                    record_temperature_samples(
                        &mut self.temp_history,
                        &self.disks,
                        &current_hms_label(),
                    );

                    if let Some((node, dtype)) = prev_sel {
                        self.selected_idx = find_selected_disk_index(&self.disks, &node, &dtype);
                    } else {
                        self.selected_idx = 0;
                    }
                    if self.selected_idx >= self.disks.len() {
                        self.selected_idx = 0;
                    }
                }
                WorkerEvent::AsciiCompleted {
                    device_node,
                    result,
                } => {
                    if self.dialogs.ascii_device == device_node {
                        self.dialogs.ascii_loading = false;
                        match result {
                            Ok(bytes) => {
                                self.dialogs.ascii_hex_text = hex_dump(&bytes);
                                self.dialogs.ascii_raw_bytes = bytes;
                            }
                            Err(e) => {
                                self.dialogs.ascii_error = Some(e);
                            }
                        }
                    }
                }
                WorkerEvent::SelfTestCompleted {
                    device_node,
                    device_type,
                    result,
                } => match result {
                    Ok(res) => {
                        let ok = res.ok;
                        self.dialogs.self_test_modal = Some(SelfTestModalState {
                            device_node,
                            device_type,
                            result: res,
                        });
                        if ok {
                            self.trigger_scan(ctx, self.should_auto_elevate());
                        }
                    }
                    Err(e) => {
                        self.set_toast(format!("{}: {}", tr.toast_selftest_error, e));
                    }
                },
                WorkerEvent::SelfTestAborted(res) => match res {
                    Ok(_) => {
                        self.set_toast(tr.selftest_aborted_msg.to_string());
                        self.trigger_scan(ctx, self.should_auto_elevate());
                    }
                    Err(e) => {
                        self.set_toast(format!("{}: {}", tr.toast_selftest_abort_failed, e));
                    }
                },
                WorkerEvent::Toast(msg) => {
                    self.set_toast(msg);
                }
            }
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context, tr: &Strings) {
        let refresh = ctx.input(|i| {
            i.key_pressed(egui::Key::F5) || (i.modifiers.ctrl && i.key_pressed(egui::Key::R))
        });
        if refresh {
            self.trigger_scan(ctx, self.should_auto_elevate());
        }

        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::G)) {
            self.dialogs.grid_open = !self.dialogs.grid_open;
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Q)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::T)) {
            self.save_selected_disk_json(tr);
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::S)) {
            let short = self
                .disks
                .get(self.selected_idx)
                .map(|d| disk_filename_stem(&self.disks, d))
                .unwrap_or_else(|| "window".to_string());
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("PNG", &["png"])
                .set_file_name(format!("RDiskInfo_{}.png", short))
                .save_file()
            {
                self.pending_screenshot = Some((path, 1));
                ctx.request_repaint();
            }
        }

        // Mouse forward / back buttons (Extra1 / Extra2) and Alt+Left/Right
        let total = self.disks.len();
        if total > 1 {
            let back = ctx.input(|i| {
                i.pointer.button_clicked(egui::PointerButton::Extra1)
                    || (i.modifiers.alt && i.key_pressed(egui::Key::ArrowLeft))
            });
            let fwd = ctx.input(|i| {
                i.pointer.button_clicked(egui::PointerButton::Extra2)
                    || (i.modifiers.alt && i.key_pressed(egui::Key::ArrowRight))
            });
            if back && (self.selected_idx > 0 || self.config.cyclic_navigation) {
                self.selected_idx = if self.selected_idx == 0 {
                    total - 1
                } else {
                    self.selected_idx - 1
                };
            }
            if fwd && (self.selected_idx + 1 < total || self.config.cyclic_navigation) {
                self.selected_idx = (self.selected_idx + 1) % total;
            }
        }
    }

    fn show_menu_bar(&mut self, ui: &mut egui::Ui, tr: &Strings) {
        let ctx = ui.ctx().clone();
        egui::MenuBar::new().ui(ui, |ui| {
            // File menu
            ui.menu_button(tr.menu_file, |ui| {
                let has_disk = !self.disks.is_empty();
                if ui
                    .add_enabled(
                        has_disk,
                        egui::Button::new(tr.save_json).shortcut_text("Ctrl+T"),
                    )
                    .clicked()
                {
                    self.save_selected_disk_json(tr);
                    ui.close();
                }

                if ui
                    .add_enabled(has_disk, egui::Button::new(tr.save_txt_report))
                    .clicked()
                {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Text Report", &["txt"])
                        .set_file_name("RDiskInfo_Report.txt")
                        .save_file()
                    {
                        let report = format_full_report(&self.disks, &self.config, tr);
                        match std::fs::write(&path, report) {
                            Ok(()) => {
                                self.set_toast(format!("{}: {}", tr.toast_saved, path.display()))
                            }
                            Err(e) => self.set_toast(format!("{}: {}", tr.toast_error, e)),
                        }
                    }
                    ui.close();
                }

                if ui
                    .add(egui::Button::new(tr.save_image).shortcut_text("Ctrl+S"))
                    .clicked()
                {
                    let short = self
                        .disks
                        .get(self.selected_idx)
                        .map(|d| disk_filename_stem(&self.disks, d))
                        .unwrap_or_else(|| "window".to_string());
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("PNG", &["png"])
                        .set_file_name(format!("RDiskInfo_{}.png", short))
                        .save_file()
                    {
                        self.pending_screenshot = Some((path, 2));
                        ctx.request_repaint();
                    }
                    ui.close();
                }

                if ui
                    .add_enabled(has_disk, egui::Button::new(tr.copy_info))
                    .clicked()
                {
                    if let Some(disk) = self.disks.get(self.selected_idx) {
                        let txt = format_single_disk_report(disk, &self.config, tr);
                        ctx.copy_text(txt);
                        self.set_toast(tr.copied_notice.to_string());
                    }
                    ui.close();
                }

                ui.separator();
                if ui
                    .add(egui::Button::new(tr.quit).shortcut_text("Ctrl+Q"))
                    .clicked()
                {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });

            // Device menu
            ui.menu_button(tr.menu_device, |ui| {
                if ui
                    .add(egui::Button::new(tr.refresh_devices).shortcut_text("F5"))
                    .clicked()
                {
                    self.trigger_scan(&ctx, self.should_auto_elevate());
                    ui.close();
                }

                ui.menu_button(tr.auto_refresh, |ui| {
                    for (mins, lbl) in [
                        (0u32, tr.off.to_string()),
                        (1, format!("1 {}", tr.minutes_short)),
                        (5, format!("5 {}", tr.minutes_short)),
                        (10, format!("10 {}", tr.minutes_short)),
                        (30, format!("30 {}", tr.minutes_short)),
                    ] {
                        if ui
                            .radio_value(&mut self.config.auto_refresh_minutes, mins, lbl)
                            .clicked()
                        {
                            self.config.save();
                            ui.close();
                        }
                    }
                });

                if matches!(
                    self.elevation,
                    ElevationState::Denied { .. }
                        | ElevationState::Unavailable { .. }
                        | ElevationState::NotRequested
                ) && ui.button(tr.elevate_privileges).clicked()
                {
                    self.trigger_scan(&ctx, true);
                    ui.close();
                }

                ui.separator();

                let selected_disk = self.disks.get(self.selected_idx).cloned();
                if ui
                    .add_enabled(selected_disk.is_some(), egui::Button::new(tr.ascii_view))
                    .clicked()
                {
                    if let Some(d) = &selected_disk {
                        self.trigger_ascii_view(&ctx, d.device_node.clone());
                    }
                    ui.close();
                }

                ui.separator();

                if let Some(d) = &selected_disk {
                    let can_ata =
                        d.protocol == DiskProtocol::Ata && !d.self_test_polling_minutes.is_empty();
                    let can_nvme = d.protocol == DiskProtocol::Nvme && d.nvme_self_test_supported;

                    ui.add_enabled_ui(can_ata || can_nvme, |ui| {
                        ui.menu_button(tr.start_self_test, |ui| {
                            if can_ata {
                                for (mode, mins) in &d.self_test_polling_minutes {
                                    let lbl = match mode.as_str() {
                                        "short" => format!(
                                            "{} ({} {})",
                                            tr.test_short, mins, tr.minutes_short
                                        ),
                                        "conveyance" => format!(
                                            "{} ({} {})",
                                            tr.test_conveyance, mins, tr.minutes_short
                                        ),
                                        "extended" => format!(
                                            "{} ({} {})",
                                            tr.test_extended, mins, tr.minutes_short
                                        ),
                                        other => {
                                            format!("{} ({} {})", other, mins, tr.minutes_short)
                                        }
                                    };
                                    if ui.button(lbl).clicked() {
                                        let arg = if mode == "extended" {
                                            "long"
                                        } else {
                                            mode.as_str()
                                        };
                                        self.trigger_self_test(
                                            &ctx,
                                            d.device_node.clone(),
                                            d.device_type.clone(),
                                            arg,
                                        );
                                        ui.close();
                                    }
                                }
                            } else if can_nvme {
                                if ui.button(tr.test_short).clicked() {
                                    self.trigger_self_test(
                                        &ctx,
                                        d.device_node.clone(),
                                        d.device_type.clone(),
                                        "short",
                                    );
                                    ui.close();
                                }
                                if ui.button(tr.test_extended).clicked() {
                                    self.trigger_self_test(
                                        &ctx,
                                        d.device_node.clone(),
                                        d.device_type.clone(),
                                        "long",
                                    );
                                    ui.close();
                                }
                            }
                            ui.separator();
                            if ui.button(tr.abort_self_test).clicked() {
                                self.trigger_abort_self_test(
                                    &ctx,
                                    d.device_node.clone(),
                                    d.device_type.clone(),
                                );
                                ui.close();
                            }
                        });
                    });

                    if ui
                        .add_enabled(
                            !d.self_test_log.is_empty(),
                            egui::Button::new(tr.self_test_log),
                        )
                        .clicked()
                    {
                        self.dialogs.self_test_log_open = true;
                        ui.close();
                    }
                }
            });

            // Settings menu
            ui.menu_button(tr.menu_settings, |ui| {
                if ui
                    .checkbox(&mut self.config.ignore_c4, tr.ignore_c4)
                    .changed()
                {
                    self.reevaluate_all_disks();
                    self.config.save();
                }
                if ui.checkbox(&mut self.config.hex_raw, tr.hex_raw).changed() {
                    self.config.save();
                }
                if ui
                    .checkbox(&mut self.config.use_fahrenheit, tr.use_fahrenheit)
                    .changed()
                {
                    self.config.save();
                }
                if ui
                    .checkbox(&mut self.config.cyclic_navigation, tr.cyclic_navigation)
                    .changed()
                {
                    self.config.save();
                }
                if ui
                    .checkbox(
                        &mut self.config.use_gb_instead_of_tb,
                        tr.use_gb_instead_of_tb,
                    )
                    .changed()
                {
                    self.config.save();
                }
                if ui
                    .checkbox(
                        &mut self.config.hide_serial_by_default,
                        tr.hide_serial_default,
                    )
                    .changed()
                {
                    self.serial_revealed = false;
                    self.config.save();
                }

                ui.menu_button(tr.ssd_wear_threshold, |ui| {
                    for pct in [5u8, 10, 15, 20, 25] {
                        if ui
                            .radio_value(
                                &mut self.config.ssd_wear_warning_percent,
                                pct,
                                format!("<= {} %", pct),
                            )
                            .changed()
                        {
                            self.reevaluate_all_disks();
                            self.config.save();
                            ui.close();
                        }
                    }
                });

                if ui.button(tr.temp_thresholds_menu).clicked() {
                    self.dialogs.temp_settings_open = true;
                    ui.close();
                }

                ui.separator();
                if ui.button(tr.clear_settings).clicked() {
                    self.config.reset();
                    self.custom_bg_texture = None;
                    self.reevaluate_all_disks();
                    let palette =
                        ThemePalette::from_kind(self.config.theme, self.config.panel_opacity);
                    palette.apply_to_ctx(&ctx);
                    ui.close();
                }
            });

            // Theme menu
            ui.menu_button(tr.menu_theme, |ui| {
                for &kind in ThemeKind::ALL {
                    if ui
                        .radio_value(&mut self.config.theme, kind, kind.localized_name(tr))
                        .clicked()
                    {
                        self.config.save();
                        let palette =
                            ThemePalette::from_kind(self.config.theme, self.config.panel_opacity);
                        palette.apply_to_ctx(&ctx);
                        ui.close();
                    }
                }
                ui.separator();
                if ui
                    .checkbox(&mut self.config.show_theme_artwork, tr.show_theme_artwork)
                    .changed()
                {
                    self.config.save();
                }
                if ui.button(tr.load_custom_bg).clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
                        .pick_file()
                    {
                        if let Some(tex) = load_texture_from_path(&ctx, &path) {
                            self.custom_bg_texture = Some(tex);
                            self.config.custom_background_path =
                                Some(path.to_string_lossy().to_string());
                            self.config.save();
                        } else {
                            self.set_toast(tr.toast_image_load_failed.to_string());
                        }
                    }
                    ui.close();
                }
                if self.custom_bg_texture.is_some() && ui.button(tr.clear_custom_bg).clicked() {
                    self.custom_bg_texture = None;
                    self.config.custom_background_path = None;
                    self.config.save();
                    ui.close();
                }
                ui.label(egui::RichText::new(tr.bg_opacity).size(11.0));
                if ui
                    .add(egui::Slider::new(
                        &mut self.config.panel_opacity,
                        0.55..=1.0,
                    ))
                    .changed()
                {
                    self.config.save();
                }
            });

            // Disk menu
            ui.menu_button(tr.menu_disk, |ui| {
                if ui
                    .add(egui::Button::new(tr.grid_view).shortcut_text("Ctrl+G"))
                    .clicked()
                {
                    self.dialogs.grid_open = !self.dialogs.grid_open;
                    ui.close();
                }
                if !self.disks.is_empty() {
                    ui.separator();
                    for (idx, d) in self.disks.iter().enumerate() {
                        let cap =
                            format_capacity(d.capacity_bytes, self.config.use_gb_instead_of_tb);
                        let duplicate_node = self
                            .disks
                            .iter()
                            .filter(|other| other.device_node == d.device_node)
                            .count()
                            > 1;
                        let label = if duplicate_node && !d.device_type.is_empty() {
                            format!(
                                "({}) {} {} [{}]",
                                idx + 1,
                                d.display_title(),
                                cap,
                                d.device_type
                            )
                        } else {
                            format!("({}) {} {}", idx + 1, d.display_title(), cap)
                        };
                        if ui.radio_value(&mut self.selected_idx, idx, label).clicked() {
                            ui.close();
                        }
                    }
                }
            });

            // Language menu
            ui.menu_button(tr.menu_language, |ui| {
                for &lang_choice in LanguageChoice::ALL {
                    if ui
                        .radio_value(
                            &mut self.config.language,
                            lang_choice,
                            lang_choice.localized_label(tr),
                        )
                        .clicked()
                    {
                        self.config.save();
                        ui.close();
                    }
                }
            });

            // Help menu
            ui.menu_button(tr.menu_help, |ui| {
                if ui.button(tr.about_app).clicked() {
                    self.dialogs.about_open = true;
                    ui.close();
                }
            });

            // Right-side scanning indicator / toast
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.scanning {
                    ui.spinner();
                    ui.label(egui::RichText::new(tr.scanning_disks).size(11.5));
                } else if let Some((msg, at)) = &self.toast_msg
                    && at.elapsed() < Duration::from_secs(6)
                {
                    ui.label(egui::RichText::new(msg).size(11.5));
                }
            });
        });
    }
}

impl eframe::App for RDiskInfoApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let lang = self.config.language.resolve();
        let tr = Strings::for_language(lang);

        self.poll_events(&ctx, tr);
        self.handle_shortcuts(&ctx, tr);

        // Delayed screenshot capture (waits 2 frames so open menus cleanly vanish)
        if let Some((path, frames_left)) = self.pending_screenshot.take() {
            if frames_left == 0 {
                let tx = self.tx.clone();
                let ctx_cb = ctx.clone();
                let saved_lbl = tr.toast_screenshot_saved;
                let failed_lbl = tr.toast_screenshot_failed;
                ctx.request_screenshot(move |img| {
                    let msg = match save_screenshot_png(&img, &path) {
                        Ok(()) => format!("{}: {}", saved_lbl, path.display()),
                        Err(e) => format!("{}: {}", failed_lbl, e),
                    };
                    let _ = tx.send(WorkerEvent::Toast(msg));
                    ctx_cb.request_repaint();
                });
            } else {
                self.pending_screenshot = Some((path, frames_left - 1));
                ctx.request_repaint();
            }
        }

        // Toast expiration repaint
        if let Some((_, at)) = &self.toast_msg
            && at.elapsed() < Duration::from_secs(6)
        {
            ctx.request_repaint_after(Duration::from_secs(1));
        }

        // Auto-refresh timer
        if self.config.auto_refresh_minutes > 0 {
            let interval = Duration::from_secs(u64::from(self.config.auto_refresh_minutes) * 60);
            if self.last_refresh.elapsed() >= interval && !self.scanning {
                self.trigger_scan(&ctx, self.should_auto_elevate());
            }
            ctx.request_repaint_after(Duration::from_secs(5));
        }

        let palette = ThemePalette::from_kind(self.config.theme, self.config.panel_opacity);
        let opacity_key = (self.config.panel_opacity * 1000.0).round() as u32;
        if self.applied_theme_state != Some((self.config.theme, opacity_key)) {
            palette.apply_to_ctx(&ctx);
            self.applied_theme_state = Some((self.config.theme, opacity_key));
        }

        // Top menu bar
        egui::Panel::top("menu_bar_panel")
            .frame(
                egui::Frame::side_top_panel(ui.style())
                    .fill(palette.panel_bg)
                    .inner_margin(egui::Margin::symmetric(8, 4)),
            )
            .show(ui, |ui| {
                self.show_menu_bar(ui, tr);
            });

        // Main central area with themed background & optional mascot panel
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                let full_rect = ui.max_rect();
                artwork::paint_window_background(
                    ui.painter(),
                    full_rect,
                    &palette,
                    self.custom_bg_texture.as_ref(),
                    self.config.show_theme_artwork,
                );

                // Optional right mascot showcase panel on wide windows when Sakura theme is active
                let show_mascot_side = self.config.theme.has_stylized_artwork()
                    && self.config.show_theme_artwork
                    && self.custom_bg_texture.is_none()
                    && full_rect.width() >= 980.0;

                if show_mascot_side {
                    let mascot_w = 210.0;
                    let mascot_rect = egui::Rect::from_min_max(
                        egui::Pos2::new(full_rect.right() - mascot_w, full_rect.top()),
                        full_rect.right_bottom(),
                    );
                    artwork::paint_sakura_mascot_card(ui.painter(), mascot_rect, palette.is_dark);
                }

                let content_rect = if show_mascot_side {
                    egui::Rect::from_min_max(
                        full_rect.min,
                        egui::Pos2::new(full_rect.right() - 210.0, full_rect.max.y),
                    )
                } else {
                    full_rect
                };

                let mut inner_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(content_rect.shrink(8.0))
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );

                self.render_main_content(&mut inner_ui, &ctx, tr, &palette);
            });

        // Temperature & Alert Thresholds Modal
        if dialogs::show_threshold_settings_dialog(
            &ctx,
            &mut self.dialogs.temp_settings_open,
            &mut self.config,
            tr,
            &palette,
        ) {
            self.reevaluate_all_disks();
            self.config.save();
        }

        // Floating dialogs & modals
        let dialog_action = dialogs::show_dialogs(
            &ctx,
            &mut self.dialogs,
            dialogs::DialogViewContext {
                disks: &self.disks,
                selected_idx: self.selected_idx,
                cfg: &self.config,
                tr,
                palette: &palette,
                smartctl_version: self.smartctl_version.as_deref(),
                elevation: &self.elevation,
            },
        );

        if let Some(act) = dialog_action {
            match act {
                DialogAction::SelectDisk(idx) => {
                    if idx < self.disks.len() {
                        self.selected_idx = idx;
                    }
                }
                DialogAction::SaveBinary { device_node, bytes } => {
                    let short = device_node.rsplit('/').next().unwrap_or("disk");
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Binary", &["bin"])
                        .set_file_name(format!("{}.bin", short))
                        .save_file()
                    {
                        match std::fs::write(&path, bytes) {
                            Ok(()) => {
                                self.set_toast(format!("{}: {}", tr.toast_saved, path.display()))
                            }
                            Err(e) => self.set_toast(format!("{}: {}", tr.toast_error, e)),
                        }
                    }
                }
                DialogAction::AbortSelfTest {
                    device_node,
                    device_type,
                } => {
                    self.trigger_abort_self_test(&ctx, device_node, device_type);
                }
                DialogAction::CopiedNotice => {
                    self.set_toast(tr.copied_notice.to_string());
                }
            }
        }
    }
}

impl RDiskInfoApp {
    fn render_main_content(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        tr: &Strings,
        palette: &ThemePalette,
    ) {
        // Non-blocking warning/privilege banner if smartctl missing or elevation denied
        if self.smartctl_missing {
            egui::Frame::new()
                .fill(palette.caution_bg.linear_multiply(0.25))
                .stroke(egui::Stroke::new(1.0, palette.caution_orb))
                .corner_radius(egui::CornerRadius::same(6))
                .inner_margin(egui::Margin::symmetric(10, 5))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(format!("⚠ {}", tr.smartctl_missing_msg)).strong(),
                    );
                });
            ui.add_space(4.0);
        } else if !self.scanning
            && matches!(
                self.elevation,
                ElevationState::Denied { .. }
                    | ElevationState::Unavailable { .. }
                    | ElevationState::NotRequested
            )
        {
            let detail = match &self.elevation {
                ElevationState::Denied { message } | ElevationState::Unavailable { message } => {
                    Some(message.clone())
                }
                _ => None,
            };
            let mut retry_clicked = false;
            egui::Frame::new()
                .fill(palette.caution_bg.linear_multiply(0.20))
                .stroke(egui::Stroke::new(1.0, palette.caution_orb))
                .corner_radius(egui::CornerRadius::same(6))
                .inner_margin(egui::Margin::symmetric(10, 4))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        let lbl =
                            ui.label(egui::RichText::new(format!("🔒 {}", tr.root_needed_banner)));
                        if let Some(d) = detail.as_deref() {
                            lbl.on_hover_text(d);
                        }
                        if ui.button(tr.retry_elevation).clicked() {
                            retry_clicked = true;
                        }
                    });
                });
            if retry_clicked {
                self.trigger_scan(ctx, true);
            }
            ui.add_space(4.0);
        }

        if self.disks.is_empty() {
            ui.centered_and_justified(|ui| {
                if self.scanning {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.heading(tr.scanning_disks);
                    });
                } else {
                    ui.heading(tr.no_disks_found);
                }
            });
            return;
        }

        if self.selected_idx >= self.disks.len() {
            self.selected_idx = 0;
        }

        // 1. Top disk cards bar
        disk_bar::show_disk_switcher(
            ui,
            &self.disks,
            &mut self.selected_idx,
            &self.config,
            tr,
            palette,
        );
        ui.add_space(6.0);

        // 2. Navigation header `< Model : Capacity >`
        disk_bar::show_navigation_header(
            ui,
            &self.disks,
            &mut self.selected_idx,
            &self.config,
            palette,
        );
        ui.add_space(6.0);

        // 3. Summary Panel (Health + Temperature + Technical Fields)
        let mut toast_out = None;
        summary_panel::show_summary_panel(
            ui,
            &self.disks[self.selected_idx],
            &self.config,
            tr,
            palette,
            &mut self.serial_revealed,
            &mut toast_out,
        );
        if let Some(t) = toast_out {
            self.set_toast(t);
        }

        ui.add_space(8.0);

        // 4. Tabs Selector Bar
        ui.horizontal_wrapped(|ui| {
            for (tab, label) in [
                (DetailTab::Smart, tr.tab_smart),
                (DetailTab::Hardware, tr.tab_hardware),
                (DetailTab::Partitions, tr.tab_partitions),
                (DetailTab::Diagnostics, tr.tab_diagnostics),
                (DetailTab::Raw, tr.tab_raw),
            ] {
                let selected = self.active_tab == tab;
                let text = if selected {
                    egui::RichText::new(label).strong()
                } else {
                    egui::RichText::new(label)
                };
                if ui.selectable_label(selected, text).clicked() {
                    self.active_tab = tab;
                }
            }
        });
        ui.separator();

        // 5. Active Tab Body (fills remaining space)
        let selected_disk = &self.disks[self.selected_idx];
        let mut self_test_req = None;
        let device_node_for_req = selected_disk.device_node.clone();
        let device_type_for_req = selected_disk.device_type.clone();
        let temp_samples: Vec<TempSample> = self
            .temp_history
            .get(&(device_node_for_req.clone(), device_type_for_req.clone()))
            .cloned()
            .unwrap_or_default();

        let mut raw_copied = false;
        egui::Frame::new()
            .fill(palette.panel_bg)
            .stroke(egui::Stroke::new(1.0, palette.border))
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                let target_h = (ui.available_height() - 4.0).max(80.0);
                ui.set_min_height(target_h);
                ui.set_max_height(target_h);
                match self.active_tab {
                    DetailTab::Smart => {
                        smart_table::show_smart_table(
                            ui,
                            selected_disk,
                            &self.config,
                            tr,
                            palette,
                            &mut self.smart_filter,
                        );
                    }
                    DetailTab::Hardware => {
                        details_tabs::show_hardware_tab(
                            ui,
                            selected_disk,
                            &self.config,
                            tr,
                            palette,
                        );
                    }
                    DetailTab::Partitions => {
                        details_tabs::show_partitions_tab(
                            ui,
                            selected_disk,
                            &self.config,
                            tr,
                            palette,
                        );
                    }
                    DetailTab::Diagnostics => {
                        self_test_req = details_tabs::show_diagnostics_tab_with_history(
                            ui,
                            selected_disk,
                            &temp_samples,
                            &self.config,
                            tr,
                            palette,
                        );
                    }
                    DetailTab::Raw => {
                        raw_copied = details_tabs::show_raw_tab(
                            ui,
                            selected_disk,
                            tr,
                            palette,
                            &mut self.raw_show_json,
                            &mut self.raw_filter,
                        );
                    }
                }
            });

        if raw_copied {
            self.set_toast(tr.copied_notice.to_string());
        }

        if let Some(req) = self_test_req {
            match req {
                SelfTestActionRequest::StartShort => {
                    self.trigger_self_test(ctx, device_node_for_req, device_type_for_req, "short")
                }
                SelfTestActionRequest::StartExtended => {
                    self.trigger_self_test(ctx, device_node_for_req, device_type_for_req, "long")
                }
                SelfTestActionRequest::StartConveyance => self.trigger_self_test(
                    ctx,
                    device_node_for_req,
                    device_type_for_req,
                    "conveyance",
                ),
                SelfTestActionRequest::Abort => {
                    self.trigger_abort_self_test(ctx, device_node_for_req, device_type_for_req)
                }
            }
        }
    }
}

pub fn find_selected_disk_index(disks: &[DiskInfo], prev_node: &str, prev_type: &str) -> usize {
    if let Some(pos) = disks
        .iter()
        .position(|d| d.device_node == prev_node && d.device_type == prev_type)
    {
        return pos;
    }
    let same_node: Vec<usize> = disks
        .iter()
        .enumerate()
        .filter(|(_, d)| d.device_node == prev_node)
        .map(|(idx, _)| idx)
        .collect();
    if same_node.len() == 1 {
        return same_node[0];
    }
    0
}

pub fn record_temperature_samples(
    history: &mut HashMap<(String, String), Vec<TempSample>>,
    disks: &[DiskInfo],
    label: &str,
) {
    for disk in disks {
        let Some(c) = disk.temperature.current_celsius else {
            continue;
        };
        if c <= 0 || c >= 300 {
            continue;
        }
        let key = (disk.device_node.clone(), disk.device_type.clone());
        if !history.contains_key(&key)
            && disks
                .iter()
                .filter(|d| d.device_node == disk.device_node)
                .count()
                == 1
            && let Some(old_key) = history
                .keys()
                .find(|(n, _)| n == &disk.device_node)
                .cloned()
            && let Some(old_samples) = history.remove(&old_key)
        {
            history.insert(key.clone(), old_samples);
        }
        let samples = history.entry(key).or_default();
        samples.push(TempSample {
            label: label.to_string(),
            celsius: c,
        });
        if samples.len() > 120 {
            let excess = samples.len() - 120;
            samples.drain(0..excess);
        }
    }
}

fn current_hms_label() -> String {
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let res = unsafe { libc::localtime_r(&now, &mut tm) };
    if res.is_null() {
        format!("{}s", now.max(0))
    } else {
        format!("{:02}:{:02}:{:02}", tm.tm_hour, tm.tm_min, tm.tm_sec)
    }
}

fn disk_filename_stem(disks: &[DiskInfo], disk: &DiskInfo) -> String {
    let base = disk.device_node.rsplit('/').next().unwrap_or("disk");
    let duplicate = disks
        .iter()
        .filter(|other| other.device_node == disk.device_node)
        .count()
        > 1;
    if duplicate && !disk.device_type.is_empty() {
        let safe_type: String = disk
            .device_type
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        format!("{}_{}", base, safe_type)
    } else {
        base.to_string()
    }
}

fn load_texture_from_path(ctx: &egui::Context, path: &Path) -> Option<egui::TextureHandle> {
    let img = image::open(path).ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let color_img = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    Some(ctx.load_texture("custom_bg", color_img, egui::TextureOptions::LINEAR))
}
