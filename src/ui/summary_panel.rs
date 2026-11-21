use crate::config::AppConfig;
use crate::i18n::Strings;
use crate::model::{DiskInfo, HealthStatus, format_data_volume, format_temperature, temp_status};
use crate::theme::ThemePalette;
use egui::{CornerRadius, RichText, Stroke, Ui, Vec2};

pub fn show_summary_panel(
    ui: &mut Ui,
    disk: &DiskInfo,
    cfg: &AppConfig,
    tr: &Strings,
    palette: &ThemePalette,
    serial_revealed: &mut bool,
    copied_toast: &mut Option<String>,
) {
    ui.horizontal_top(|ui| {
        // Left column: Prominent Health & Temperature badges (QDiskInfo + CrystalDiskInfo style)
        ui.vertical(|ui| {
            ui.set_width(142.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(tr.health_status)
                        .size(12.0)
                        .color(palette.text_secondary),
                );
            });

            let (h_bg, h_fg, _) = palette.status_colors(disk.health_status);
            let health_main = tr.health_label(disk.health_status);
            let health_sub = disk.health_percentage.map(|p| format!("{} %", p));

            let health_frame = egui::Frame::new()
                .fill(h_bg)
                .stroke(Stroke::new(1.2, palette.border))
                .corner_radius(CornerRadius::same(7))
                .inner_margin(egui::Margin::symmetric(8, 8));

            let health_resp = health_frame
                .show(ui, |ui| {
                    ui.set_width(124.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(health_main).size(15.5).strong().color(h_fg));
                        if let Some(pct) = health_sub {
                            ui.label(RichText::new(pct).size(14.5).strong().color(h_fg));
                        }
                    });
                })
                .response;

            if !disk.health_reasons.is_empty() {
                health_resp.on_hover_text(disk.health_reasons.join("\n"));
            }

            ui.add_space(6.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(tr.temperature)
                        .size(12.0)
                        .color(palette.text_secondary),
                );
            });

            let t_stat = temp_status(&disk.temperature);
            let (t_bg, t_fg, _) = palette.status_colors(t_stat);
            let temp_str = format_temperature(disk.temperature.current_celsius, cfg.use_fahrenheit);

            let temp_frame = egui::Frame::new()
                .fill(t_bg)
                .stroke(Stroke::new(1.2, palette.border))
                .corner_radius(CornerRadius::same(7))
                .inner_margin(egui::Margin::symmetric(8, 7));

            let temp_resp = temp_frame
                .show(ui, |ui| {
                    ui.set_width(124.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(temp_str).size(15.5).strong().color(t_fg));
                    });
                })
                .response;

            if let Some(tip) = build_temp_tooltip(disk, cfg, tr) {
                temp_resp.on_hover_text(tip);
            }
        });

        ui.add_space(8.0);

        // Right side: Two balanced columns of technical specs + Features footer
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 3.0;
            let total_w = ui.available_width();
            let col_w = ((total_w - 14.0) * 0.5).max(210.0);

            ui.horizontal_top(|ui| {
                // Middle Column (Identity & Interface)
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 3.0;
                    ui.set_width(col_w);
                    spec_row(ui, tr.firmware, &disk.firmware_version, false, palette);
                    serial_row(
                        ui,
                        tr,
                        &disk.serial_number,
                        cfg.hide_serial_by_default,
                        serial_revealed,
                        palette,
                        copied_toast,
                    );
                    spec_row(
                        ui,
                        tr.interface_label,
                        &disk.interface_label,
                        false,
                        palette,
                    );
                    spec_row(ui, tr.transfer_mode, &disk.transfer_mode, false, palette);
                    let proto_type = if disk.device_type.is_empty() {
                        disk.protocol_str.clone()
                    } else {
                        format!("{} ({})", disk.protocol_str, disk.device_type)
                    };
                    spec_row(ui, tr.protocol, &proto_type, false, palette);
                    let node_display = match &disk.block_device_node {
                        Some(blk) if blk != &disk.device_node => {
                            format!("{} ({})", disk.device_node, blk)
                        }
                        _ => disk.device_node.clone(),
                    };
                    spec_row(ui, tr.device_node, &node_display, false, palette);
                    spec_row(ui, tr.standard, &disk.standard, false, palette);
                });

                ui.add_space(10.0);

                // Right Column (Usage, Endurance & Mounts)
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 3.0;
                    ui.set_width(col_w);
                    let reads =
                        format_data_volume(disk.total_host_reads_bytes, cfg.use_gb_instead_of_tb);
                    let writes =
                        format_data_volume(disk.total_host_writes_bytes, cfg.use_gb_instead_of_tb);
                    spec_row(ui, tr.total_reads, &reads, true, palette);
                    spec_row(ui, tr.total_writes, &writes, true, palette);

                    let rot = match disk.rotation_rate {
                        Some(0) => "---- (SSD)".to_string(),
                        Some(rpm) => format!("{} RPM", rpm),
                        None if disk.sysfs.rotational == Some(true) => "---- (HDD)".to_string(),
                        None => "----".to_string(),
                    };
                    spec_row(ui, tr.rotation_rate, &rot, true, palette);

                    let p_count = disk
                        .power_cycle_count
                        .map(|c| format!("{} {}", c, tr.count_unit))
                        .unwrap_or_else(|| tr.unknown.to_string());
                    spec_row(ui, tr.power_on_count, &p_count, true, palette);

                    let p_hours = match (disk.power_on_hours, disk.power_on_minutes) {
                        (Some(h), Some(m)) if m > 0 => format!("{} {} ({}m)", h, tr.hours_unit, m),
                        (Some(h), _) => format!("{} {}", h, tr.hours_unit),
                        _ => tr.unknown.to_string(),
                    };
                    spec_row(ui, tr.power_on_hours, &p_hours, true, palette);

                    let mounts = disk.all_mountpoints();
                    let mount_str = if mounts.is_empty() {
                        "----".to_string()
                    } else {
                        mounts.join(", ")
                    };
                    spec_row(ui, tr.drive_mounts, &mount_str, true, palette);

                    let log_bs = if disk.logical_block_size > 0 {
                        disk.logical_block_size
                    } else {
                        512
                    };
                    let phys_bs = if disk.physical_block_size > 0 {
                        disk.physical_block_size
                    } else {
                        log_bs
                    };
                    let sector_str = format!("{} B / {} B", log_bs, phys_bs);
                    spec_row(ui, tr.sector_size, &sector_str, true, palette);
                });
            });

            ui.add_space(3.0);
            // Features & Form Factor / WWN / NAND Writes bar (CrystalDiskInfo style!)
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(format!("{}:", tr.features))
                        .size(12.0)
                        .color(palette.text_secondary),
                );
                if disk.features.is_empty() {
                    ui.label(
                        RichText::new("----")
                            .size(12.0)
                            .color(palette.text_secondary),
                    );
                } else {
                    for feat in &disk.features {
                        egui::Frame::new()
                            .fill(palette.field_bg)
                            .stroke(Stroke::new(1.0, palette.border))
                            .corner_radius(CornerRadius::same(4))
                            .inner_margin(egui::Margin::symmetric(6, 2))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(feat).size(11.0).color(palette.text_primary),
                                );
                            });
                    }
                }
                if let Some(nand) = disk.total_nand_writes_bytes {
                    ui.label(
                        RichText::new(format!(
                            "• {}: {}",
                            tr.nand_writes,
                            format_data_volume(Some(nand), cfg.use_gb_instead_of_tb)
                        ))
                        .size(11.5)
                        .color(palette.text_secondary),
                    );
                }
                if let Some(us) = disk.unsafe_shutdowns {
                    ui.label(
                        RichText::new(format!("• {}: {}", tr.unsafe_shutdowns, us))
                            .size(11.5)
                            .color(palette.text_secondary),
                    );
                }
                if let Some(ff) = &disk.form_factor {
                    ui.label(
                        RichText::new(format!("• {}", ff))
                            .size(11.5)
                            .color(palette.text_secondary),
                    );
                }
                if let Some(wwn) = &disk.wwn {
                    ui.label(
                        RichText::new(format!("• WWN: {}", wwn))
                            .size(11.5)
                            .monospace()
                            .color(palette.text_secondary),
                    );
                }
            });

            if !disk.health_reasons.is_empty() && disk.health_status != HealthStatus::Good {
                ui.add_space(3.0);
                let (r_bg, r_fg, _) = palette.status_colors(disk.health_status);
                egui::Frame::new()
                    .fill(r_bg.linear_multiply(0.22))
                    .stroke(Stroke::new(1.0, r_bg))
                    .corner_radius(CornerRadius::same(5))
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(format!("⚠ {}", disk.health_reasons.join("  |  ")))
                                .size(11.5)
                                .color(if palette.is_dark { r_bg } else { r_fg }),
                        );
                    });
            }
        });
    });
}

fn spec_row(
    ui: &mut Ui,
    label: &str,
    value: &str,
    right_align_value: bool,
    palette: &ThemePalette,
) {
    let val_display = if value.trim().is_empty() {
        "----"
    } else {
        value
    };
    ui.horizontal(|ui| {
        let label_w = 124.0;
        ui.allocate_ui_with_layout(
            Vec2::new(label_w, 20.0),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(label)
                            .size(11.8)
                            .color(palette.text_secondary),
                    )
                    .truncate(),
                )
                .on_hover_text(label);
            },
        );
        let box_w = ui.available_width().max(80.0);
        egui::Frame::new()
            .fill(palette.field_bg)
            .stroke(Stroke::new(1.0, palette.border))
            .corner_radius(CornerRadius::same(4))
            .inner_margin(egui::Margin::symmetric(7, 2))
            .show(ui, |ui| {
                let layout = if right_align_value {
                    egui::Layout::right_to_left(egui::Align::Center)
                } else {
                    egui::Layout::left_to_right(egui::Align::Center)
                };
                ui.allocate_ui_with_layout(
                    Vec2::new((box_w - 16.0).max(40.0), 16.0),
                    layout,
                    |ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(val_display)
                                    .size(12.0)
                                    .color(palette.text_primary),
                            )
                            .truncate(),
                        )
                        .on_hover_text(val_display);
                    },
                );
            });
    });
}

fn serial_row(
    ui: &mut Ui,
    tr: &Strings,
    serial: &str,
    hide_default: bool,
    revealed: &mut bool,
    palette: &ThemePalette,
    copied_toast: &mut Option<String>,
) {
    let is_shown = !hide_default || *revealed;
    let display = if serial.is_empty() {
        "----".to_string()
    } else if is_shown {
        serial.to_string()
    } else {
        "•".repeat(serial.chars().count().clamp(8, 16))
    };

    ui.horizontal(|ui| {
        let label_w = 124.0;
        ui.allocate_ui_with_layout(
            Vec2::new(label_w, 20.0),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(tr.serial_number)
                            .size(11.8)
                            .color(palette.text_secondary),
                    )
                    .truncate(),
                )
                .on_hover_text(tr.serial_number);
            },
        );
        let box_w = ui.available_width().max(80.0);
        egui::Frame::new()
            .fill(palette.field_bg)
            .stroke(Stroke::new(1.0, palette.border))
            .corner_radius(CornerRadius::same(4))
            .inner_margin(egui::Margin::symmetric(7, 1))
            .show(ui, |ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new((box_w - 16.0).max(40.0), 18.0),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        // Place buttons FIRST in right_to_left layout so they are never pushed off-screen
                        if !serial.is_empty() {
                            if ui
                                .small_button("📋")
                                .on_hover_text(tr.copy_serial_tip)
                                .clicked()
                            {
                                ui.ctx().copy_text(serial.to_string());
                                *copied_toast = Some(tr.serial_copied.to_string());
                            }
                            if hide_default {
                                let eye_lbl = if *revealed { "🙈" } else { "👁" };
                                if ui
                                    .small_button(eye_lbl)
                                    .on_hover_text(tr.toggle_serial_tip)
                                    .clicked()
                                {
                                    *revealed = !*revealed;
                                }
                            }
                        }
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            let label_resp = ui.add(
                                egui::Label::new(
                                    RichText::new(&display)
                                        .size(12.0)
                                        .monospace()
                                        .color(palette.text_primary),
                                )
                                .truncate(),
                            );
                            if is_shown && !serial.is_empty() {
                                label_resp.on_hover_text(serial);
                            }
                        });
                    },
                );
            });
    });
}

fn build_temp_tooltip(disk: &DiskInfo, cfg: &AppConfig, tr: &Strings) -> Option<String> {
    let mut lines = Vec::new();
    if let (Some(min), Some(max)) = (disk.temperature.lifetime_min, disk.temperature.lifetime_max) {
        lines.push(format!(
            "{}: {} / {}",
            tr.lbl_lifetime_minmax,
            format_temperature(Some(min), cfg.use_fahrenheit),
            format_temperature(Some(max), cfg.use_fahrenheit)
        ));
    }
    if let (Some(min), Some(max)) = (
        disk.temperature.power_cycle_min,
        disk.temperature.power_cycle_max,
    ) {
        lines.push(format!(
            "{}: {} / {}",
            tr.lbl_power_cycle_minmax,
            format_temperature(Some(min), cfg.use_fahrenheit),
            format_temperature(Some(max), cfg.use_fahrenheit)
        ));
    }
    lines.push(format!(
        "{}: {} / {}",
        tr.lbl_warning_crit,
        format_temperature(Some(disk.temperature.warning_threshold), cfg.use_fahrenheit),
        format_temperature(
            Some(disk.temperature.critical_threshold),
            cfg.use_fahrenheit
        )
    ));
    for s in &disk.temperature.sensors {
        lines.push(format!(
            "{}: {}",
            s.label,
            format_temperature(Some(s.celsius.round() as i32), cfg.use_fahrenheit)
        ));
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}
