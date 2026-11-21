use crate::config::AppConfig;
use crate::i18n::Strings;
use crate::model::{DiskInfo, HealthStatus, format_capacity, format_temperature};
use crate::privilege::{ElevationState, SelfTestOutcome, SelfTestResult};
use crate::theme::ThemePalette;
use egui::{CornerRadius, RichText, ScrollArea, Stroke, Vec2};

#[derive(Debug, Default)]
pub struct DialogStates {
    pub grid_open: bool,
    pub grid_search: String,

    pub ascii_open: bool,
    pub ascii_loading: bool,
    pub ascii_device: String,
    pub ascii_hex_text: String,
    pub ascii_raw_bytes: Vec<u8>,
    pub ascii_error: Option<String>,

    pub self_test_log_open: bool,
    pub about_open: bool,
    pub temp_settings_open: bool,

    pub self_test_modal: Option<SelfTestModalState>,
}

#[derive(Debug, Clone)]
pub struct SelfTestModalState {
    pub device_node: String,
    pub device_type: String,
    pub result: SelfTestResult,
}

pub enum DialogAction {
    SelectDisk(usize),
    SaveBinary {
        device_node: String,
        bytes: Vec<u8>,
    },
    AbortSelfTest {
        device_node: String,
        device_type: String,
    },
    CopiedNotice,
}

pub struct DialogViewContext<'a> {
    pub disks: &'a [DiskInfo],
    pub selected_idx: usize,
    pub cfg: &'a AppConfig,
    pub tr: &'a Strings,
    pub palette: &'a ThemePalette,
    pub smartctl_version: Option<&'a str>,
    pub elevation: &'a ElevationState,
}

pub fn show_threshold_settings_dialog(
    ctx: &egui::Context,
    open: &mut bool,
    cfg: &mut AppConfig,
    tr: &Strings,
    palette: &ThemePalette,
) -> bool {
    if !*open {
        return false;
    }
    let mut changed = false;
    let mut keep_open = *open;
    let mut close_clicked = false;

    egui::Window::new(tr.temp_thresholds_title)
        .open(&mut keep_open)
        .resizable(false)
        .default_width(420.0)
        .show(ctx, |ui| {
            let mut ata_warn = cfg.temp_warning_ata;
            let mut ata_crit = cfg.temp_critical_ata;
            let mut nvme_warn = cfg.temp_warning_nvme;
            let mut nvme_crit = cfg.temp_critical_nvme;
            let mut wear_pct = cfg.ssd_wear_warning_percent;

            egui::Grid::new("temp_thresholds_grid")
                .num_columns(3)
                .spacing([14.0, 8.0])
                .show(ui, |ui| {
                    ui.label(tr.ata_warning_temp);
                    if ui
                        .add(egui::Slider::new(&mut ata_warn, 25..=95).suffix(" °C"))
                        .changed()
                    {
                        cfg.set_ata_temp_thresholds(ata_warn, ata_crit);
                        ata_crit = cfg.temp_critical_ata;
                        changed = true;
                    }
                    ui.label(
                        RichText::new(if cfg.use_fahrenheit {
                            format_temperature(Some(cfg.temp_warning_ata), true)
                        } else {
                            String::new()
                        })
                        .size(11.5)
                        .color(palette.text_secondary),
                    );
                    ui.end_row();

                    ui.label(tr.ata_critical_temp);
                    if ui
                        .add(egui::Slider::new(&mut ata_crit, 30..=110).suffix(" °C"))
                        .changed()
                    {
                        cfg.set_ata_temp_thresholds(cfg.temp_warning_ata, ata_crit);
                        changed = true;
                    }
                    ui.label(
                        RichText::new(if cfg.use_fahrenheit {
                            format_temperature(Some(cfg.temp_critical_ata), true)
                        } else {
                            String::new()
                        })
                        .size(11.5)
                        .color(palette.text_secondary),
                    );
                    ui.end_row();

                    ui.label(tr.nvme_warning_temp);
                    if ui
                        .add(egui::Slider::new(&mut nvme_warn, 25..=105).suffix(" °C"))
                        .changed()
                    {
                        cfg.set_nvme_temp_thresholds(nvme_warn, nvme_crit);
                        nvme_crit = cfg.temp_critical_nvme;
                        changed = true;
                    }
                    ui.label(
                        RichText::new(if cfg.use_fahrenheit {
                            format_temperature(Some(cfg.temp_warning_nvme), true)
                        } else {
                            String::new()
                        })
                        .size(11.5)
                        .color(palette.text_secondary),
                    );
                    ui.end_row();

                    ui.label(tr.nvme_critical_temp);
                    if ui
                        .add(egui::Slider::new(&mut nvme_crit, 30..=120).suffix(" °C"))
                        .changed()
                    {
                        cfg.set_nvme_temp_thresholds(cfg.temp_warning_nvme, nvme_crit);
                        changed = true;
                    }
                    ui.label(
                        RichText::new(if cfg.use_fahrenheit {
                            format_temperature(Some(cfg.temp_critical_nvme), true)
                        } else {
                            String::new()
                        })
                        .size(11.5)
                        .color(palette.text_secondary),
                    );
                    ui.end_row();

                    ui.label(tr.ssd_wear_threshold);
                    if ui
                        .add(egui::Slider::new(&mut wear_pct, 1..=50).suffix(" %"))
                        .changed()
                    {
                        cfg.ssd_wear_warning_percent = wear_pct;
                        changed = true;
                    }
                    ui.label("");
                    ui.end_row();
                });

            ui.add_space(6.0);
            ui.label(
                RichText::new(tr.temp_thresholds_note)
                    .size(11.5)
                    .color(palette.text_secondary),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(tr.reset_defaults).clicked() {
                    cfg.reset_thresholds();
                    changed = true;
                }
                if ui.button(tr.close).clicked() {
                    close_clicked = true;
                }
            });
        });

    if close_clicked {
        keep_open = false;
    }
    *open = keep_open;
    changed
}

pub fn show_dialogs(
    ctx: &egui::Context,
    dialogs: &mut DialogStates,
    view: DialogViewContext<'_>,
) -> Option<DialogAction> {
    let DialogViewContext {
        disks,
        selected_idx,
        cfg,
        tr,
        palette,
        smartctl_version,
        elevation,
    } = view;
    let mut action = None;

    if dialogs.grid_open {
        let mut open = dialogs.grid_open;
        egui::Window::new(tr.grid_view)
            .open(&mut open)
            .default_size([620.0, 380.0])
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("🔍");
                    ui.add(
                        egui::TextEdit::singleline(&mut dialogs.grid_search)
                            .desired_width(ui.available_width() - 32.0)
                            .hint_text(tr.search_disk_placeholder),
                    );
                    if !dialogs.grid_search.is_empty() && ui.small_button("✖").clicked() {
                        dialogs.grid_search.clear();
                    }
                });
                ui.add_space(8.0);

                let q = dialogs.grid_search.trim().to_ascii_lowercase();
                let filtered: Vec<(usize, &DiskInfo)> = disks
                    .iter()
                    .enumerate()
                    .filter(|(_, d)| {
                        q.is_empty()
                            || d.device_node.to_ascii_lowercase().contains(&q)
                            || d.device_type.to_ascii_lowercase().contains(&q)
                            || d.model_name.to_ascii_lowercase().contains(&q)
                            || d.all_mountpoints()
                                .iter()
                                .any(|m| m.to_ascii_lowercase().contains(&q))
                    })
                    .collect();

                ScrollArea::vertical().show(ui, |ui| {
                    let avail_w = ui.available_width().max(180.0);
                    let card_w = 175.0;
                    let cols = ((avail_w + 10.0) / (card_w + 10.0)).floor().max(1.0) as usize;
                    egui::Grid::new("disk_grid_view_cards")
                        .spacing([10.0, 10.0])
                        .show(ui, |ui| {
                            for (i, (orig_idx, disk)) in filtered.iter().enumerate() {
                                let is_active = *orig_idx == selected_idx;
                                let duplicate_node = disks
                                    .iter()
                                    .filter(|other| other.device_node == disk.device_node)
                                    .count()
                                    > 1;
                                let node_label = if duplicate_node && !disk.device_type.is_empty() {
                                    format!("{} [{}]", disk.device_node, disk.device_type)
                                } else {
                                    disk.device_node.clone()
                                };
                                let frame = egui::Frame::new()
                                    .fill(if is_active {
                                        palette.card_selected_bg
                                    } else {
                                        palette.card_bg
                                    })
                                    .stroke(if is_active {
                                        Stroke::new(2.0, palette.accent)
                                    } else {
                                        Stroke::new(1.0, palette.border)
                                    })
                                    .corner_radius(CornerRadius::same(8))
                                    .inner_margin(egui::Margin::same(10));

                                let resp = frame
                                    .show(ui, |ui| {
                                        ui.set_width(card_w - 22.0);
                                        ui.vertical_centered(|ui| {
                                            ui.add(
                                                status_svg_image(disk.health_status)
                                                    .fit_to_exact_size(Vec2::splat(48.0)),
                                            );
                                            ui.add_space(4.0);
                                            ui.label(
                                                RichText::new(&node_label).strong().size(13.0),
                                            );
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(disk.display_title())
                                                        .size(11.5)
                                                        .color(palette.text_secondary),
                                                )
                                                .truncate(),
                                            );
                                            let (h_bg, h_fg, _) =
                                                palette.status_colors(disk.health_status);
                                            let temp = format_temperature(
                                                disk.temperature.current_celsius,
                                                cfg.use_fahrenheit,
                                            );
                                            let cap = format_capacity(
                                                disk.capacity_bytes,
                                                cfg.use_gb_instead_of_tb,
                                            );
                                            egui::Frame::new()
                                                .fill(h_bg)
                                                .corner_radius(CornerRadius::same(4))
                                                .inner_margin(egui::Margin::symmetric(6, 2))
                                                .show(ui, |ui| {
                                                    ui.label(
                                                        RichText::new(
                                                            tr.health_label(disk.health_status),
                                                        )
                                                        .size(11.0)
                                                        .strong()
                                                        .color(h_fg),
                                                    );
                                                });
                                            ui.label(
                                                RichText::new(format!("{} • {}", temp, cap))
                                                    .size(11.0)
                                                    .color(if is_active {
                                                        palette.accent
                                                    } else {
                                                        palette.text_secondary
                                                    }),
                                            );
                                        });
                                    })
                                    .response
                                    .interact(egui::Sense::click());

                                if resp.clicked() {
                                    action = Some(DialogAction::SelectDisk(*orig_idx));
                                }

                                if (i + 1) % cols == 0 {
                                    ui.end_row();
                                }
                            }
                        });
                });
            });
        dialogs.grid_open = open;
    }

    if dialogs.ascii_open {
        let mut open = dialogs.ascii_open;
        egui::Window::new(format!("{} — {}", tr.ascii_view, dialogs.ascii_device))
            .open(&mut open)
            .default_size([640.0, 480.0])
            .resizable(true)
            .show(ctx, |ui| {
                if dialogs.ascii_loading {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(tr.reading_raw_msg);
                    });
                } else if let Some(err) = &dialogs.ascii_error {
                    ui.colored_label(palette.bad_orb, err);
                } else {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!(
                                "{} {}",
                                dialogs.ascii_raw_bytes.len(),
                                tr.bytes_unit
                            ))
                            .color(palette.text_secondary),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(tr.save_binary).clicked()
                                && !dialogs.ascii_raw_bytes.is_empty()
                            {
                                action = Some(DialogAction::SaveBinary {
                                    device_node: dialogs.ascii_device.clone(),
                                    bytes: dialogs.ascii_raw_bytes.clone(),
                                });
                            }
                            if ui.button(format!("📋 {}", tr.copy_hex)).clicked() {
                                ui.ctx().copy_text(dialogs.ascii_hex_text.clone());
                                action = Some(DialogAction::CopiedNotice);
                            }
                        });
                    });
                    ui.separator();
                    ScrollArea::both().show(ui, |ui| {
                        let mut ro_ref: &str = &dialogs.ascii_hex_text;
                        ui.add(
                            egui::TextEdit::multiline(&mut ro_ref)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY),
                        );
                    });
                }
            });
        dialogs.ascii_open = open;
    }

    if dialogs.self_test_log_open {
        if let Some(disk) = disks.get(selected_idx) {
            let mut open = dialogs.self_test_log_open;
            let duplicate_node = disks
                .iter()
                .filter(|other| other.device_node == disk.device_node)
                .count()
                > 1;
            let log_dev = if duplicate_node && !disk.device_type.is_empty() {
                format!("{} [{}]", disk.device_node, disk.device_type)
            } else {
                disk.device_node.clone()
            };
            egui::Window::new(format!("{} — {}", tr.self_test_log, log_dev))
                .open(&mut open)
                .default_size([520.0, 340.0])
                .resizable(true)
                .show(ctx, |ui| {
                    if disk.self_test_log.is_empty() {
                        ui.label(
                            RichText::new(tr.no_selftest_log_msg).color(palette.text_secondary),
                        );
                    } else {
                        ScrollArea::vertical().show(ui, |ui| {
                            egui::Grid::new("popup_selftest_log_grid")
                                .num_columns(5)
                                .spacing([16.0, 5.0])
                                .striped(true)
                                .show(ui, |ui| {
                                    ui.strong("#");
                                    ui.strong(tr.col_type);
                                    ui.strong(tr.col_status);
                                    ui.strong(tr.power_on_hours);
                                    ui.strong("LBA");
                                    ui.end_row();
                                    for e in &disk.self_test_log {
                                        ui.label(
                                            e.num
                                                .map(|n| n.to_string())
                                                .unwrap_or_else(|| "-".into()),
                                        );
                                        ui.label(&e.test_type);
                                        ui.label(&e.status);
                                        ui.label(
                                            e.lifetime_hours
                                                .map(|h| h.to_string())
                                                .unwrap_or_else(|| "-".into()),
                                        );
                                        ui.label(e.lba.as_deref().unwrap_or("-"));
                                        ui.end_row();
                                    }
                                });
                        });
                    }
                });
            dialogs.self_test_log_open = open;
        } else {
            dialogs.self_test_log_open = false;
        }
    }

    if let Some(modal) = dialogs.self_test_modal.clone() {
        let mut keep_open = true;
        let title_dev = if modal.device_type.is_empty() {
            modal.device_node.clone()
        } else {
            format!("{} [{}]", modal.device_node, modal.device_type)
        };
        let localized_msg = match modal.result.outcome {
            SelfTestOutcome::Started => tr.selftest_started_msg.to_string(),
            SelfTestOutcome::AlreadyRunning => tr.selftest_already_running_msg.to_string(),
            SelfTestOutcome::Failed => {
                if modal.result.message.is_empty() {
                    tr.selftest_failed_msg.to_string()
                } else {
                    format!("{}: {}", tr.selftest_failed_msg, modal.result.message)
                }
            }
        };
        egui::Window::new(format!("{} — {}", tr.selftest_modal_title, title_dev))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_min_width(340.0);
                ui.label(RichText::new(localized_msg).strong());
                if let Some(prog) = &modal.result.progress_info {
                    let localized_prog = prog
                        .replace("remaining", tr.lbl_selftest_remaining)
                        .replace("completed", tr.lbl_selftest_completed);
                    ui.add_space(4.0);
                    ui.label(RichText::new(localized_prog).color(palette.caution_orb));
                }
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if modal.result.already_running && ui.button(tr.abort_self_test).clicked() {
                        action = Some(DialogAction::AbortSelfTest {
                            device_node: modal.device_node.clone(),
                            device_type: modal.device_type.clone(),
                        });
                        keep_open = false;
                    }
                    if ui.button(tr.close).clicked() {
                        keep_open = false;
                    }
                });
            });
        if !keep_open {
            dialogs.self_test_modal = None;
        }
    }

    if dialogs.about_open {
        let mut open = dialogs.about_open;
        egui::Window::new(tr.about_app)
            .open(&mut open)
            .resizable(false)
            .default_width(400.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Image::new(egui::include_image!("../../assets/icon.svg"))
                            .fit_to_exact_size(Vec2::splat(64.0)),
                    );
                    ui.vertical(|ui| {
                        ui.heading(format!("RDiskInfo v{}", env!("CARGO_PKG_VERSION")));
                        ui.label(RichText::new(tr.about_license).size(12.0));
                        ui.label(
                            RichText::new(tr.about_description)
                                .size(12.0)
                                .color(palette.text_secondary),
                        );
                    });
                });
                ui.separator();
                if let Some(ver) = smartctl_version {
                    ui.label(RichText::new(format!("{}: {}", tr.about_backend, ver)).size(11.5));
                }
                let elev_text = match elevation {
                    ElevationState::RootNative => tr.elev_root.to_string(),
                    ElevationState::ElevatedHelper => tr.elev_helper.to_string(),
                    ElevationState::UnprivilegedFullAccess => tr.elev_unpriv.to_string(),
                    ElevationState::Denied { message } => {
                        format!("{} — {}", tr.elev_denied, message)
                    }
                    ElevationState::Unavailable { message } => {
                        format!("{} — {}", tr.elev_unavail, message)
                    }
                    ElevationState::NotRequested => tr.elev_not_req.to_string(),
                };
                ui.label(
                    RichText::new(format!("{}: {}", tr.elevation_status_label, elev_text))
                        .size(11.5)
                        .color(palette.text_secondary),
                );
            });
        dialogs.about_open = open;
    }

    action
}

fn status_svg_image(status: HealthStatus) -> egui::Image<'static> {
    match status {
        HealthStatus::Good => {
            egui::Image::new(egui::include_image!("../../assets/status_good.svg"))
        }
        HealthStatus::Caution => {
            egui::Image::new(egui::include_image!("../../assets/status_caution.svg"))
        }
        HealthStatus::Bad => egui::Image::new(egui::include_image!("../../assets/status_bad.svg")),
        HealthStatus::Unknown => {
            egui::Image::new(egui::include_image!("../../assets/status_unknown.svg"))
        }
    }
}
