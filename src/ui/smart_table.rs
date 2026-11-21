use crate::config::AppConfig;
use crate::i18n::Strings;
use crate::model::{DiskInfo, DiskProtocol, HealthStatus, format_data_volume, format_temperature};
use crate::theme::ThemePalette;
use crate::ui::artwork::paint_status_orb;
use egui::{Align, Layout, RichText, ScrollArea, Ui};
use egui_extras::{Column, TableBuilder};

pub fn show_smart_table(
    ui: &mut Ui,
    disk: &DiskInfo,
    cfg: &AppConfig,
    tr: &Strings,
    palette: &ThemePalette,
    filter_query: &mut String,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("🔍").size(13.0));
        ui.add(
            egui::TextEdit::singleline(filter_query)
                .desired_width(240.0)
                .hint_text(tr.filter_attributes_hint),
        );
        if !filter_query.is_empty() && ui.small_button("✖").clicked() {
            filter_query.clear();
        }
    });
    ui.add_space(4.0);

    if !disk.smartctl_available_data
        && disk.ata_attributes.is_empty()
        && disk.nvme_attributes.is_empty()
        && disk.scsi_error_log.is_empty()
    {
        egui::Frame::new()
            .fill(palette.card_bg)
            .stroke(egui::Stroke::new(1.0, palette.border))
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::Margin::same(14))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(
                        disk.smartctl_error_message
                            .as_deref()
                            .unwrap_or(tr.root_needed_banner),
                    )
                    .color(palette.text_secondary),
                );
            });
        return;
    }

    let q = filter_query.trim().to_ascii_lowercase();

    match disk.protocol {
        DiskProtocol::Ata => show_ata_table(ui, disk, cfg, tr, palette, &q),
        DiskProtocol::Nvme => show_nvme_table(ui, disk, cfg, tr, palette, &q),
        DiskProtocol::Scsi => show_scsi_table(ui, disk, tr, palette, &q),
        DiskProtocol::Unknown => {
            ui.label(RichText::new(tr.no_smart_table_msg).color(palette.text_secondary));
        }
    }
}

fn show_ata_table(
    ui: &mut Ui,
    disk: &DiskInfo,
    cfg: &AppConfig,
    tr: &Strings,
    palette: &ThemePalette,
    filter: &str,
) {
    let rows: Vec<_> = disk
        .ata_attributes
        .iter()
        .filter(|a| {
            filter.is_empty()
                || format!("{:02X}", a.id)
                    .to_ascii_lowercase()
                    .contains(filter)
                || a.name.to_ascii_lowercase().contains(filter)
        })
        .collect();

    if rows.is_empty() {
        ui.label(RichText::new(tr.no_matching_attrs).color(palette.text_secondary));
        return;
    }

    let avail_h = ui.available_height().max(90.0);
    ScrollArea::horizontal()
        .id_salt(("ata_smart_hscroll", &disk.device_node, &disk.device_type))
        .show(ui, |ui| {
            TableBuilder::new(ui)
                .id_salt(("ata_smart_table", &disk.device_node, &disk.device_type))
                .striped(true)
                .resizable(true)
                .min_scrolled_height(0.0)
                .max_scroll_height(avail_h)
                .auto_shrink([false, false])
                .cell_layout(Layout::left_to_right(Align::Center))
                .column(Column::exact(28.0))
                .column(Column::initial(40.0).at_least(34.0))
                .column(Column::remainder().at_least(150.0))
                .column(Column::initial(58.0).at_least(46.0))
                .column(Column::initial(58.0).at_least(46.0))
                .column(Column::initial(68.0).at_least(50.0))
                .column(Column::initial(120.0).at_least(96.0))
                .column(Column::initial(120.0).at_least(80.0))
                .header(26.0, |mut header| {
                    header.col(|_ui| {});
                    header.col(|ui| {
                        ui.centered_and_justified(|ui| {
                            ui.strong(tr.col_id);
                        });
                    });
                    header.col(|ui| {
                        ui.strong(tr.col_attribute_name);
                    });
                    header.col(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.strong(tr.col_current);
                        });
                    });
                    header.col(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.strong(tr.col_worst);
                        });
                    });
                    header.col(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.strong(tr.col_threshold);
                        });
                    });
                    header.col(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.strong(tr.col_raw_values);
                        });
                    });
                    header.col(|ui| {
                        ui.strong(tr.col_decoded);
                    });
                })
                .body(|body| {
                    body.rows(24.0, rows.len(), |mut row| {
                        let attr = rows[row.index()];
                        let (_, _, orb_col) = palette.status_colors(attr.status);
                        row.col(|ui| {
                            let (rect, resp) = ui
                                .allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
                            paint_status_orb(ui.painter(), rect.center(), 6.0, orb_col);
                            resp.on_hover_text(tr.health_label(attr.status));
                        });
                        row.col(|ui| {
                            ui.centered_and_justified(|ui| {
                                ui.label(
                                    RichText::new(format!("{:02X}", attr.id))
                                        .monospace()
                                        .strong(),
                                );
                            });
                        });
                        row.col(|ui| {
                            let text = if attr.status != HealthStatus::Good {
                                RichText::new(&attr.name).strong().color(orb_col)
                            } else {
                                RichText::new(&attr.name)
                            };
                            ui.add(egui::Label::new(text).truncate())
                                .on_hover_text(format!(
                                    "{}: 0x{:02X} ({})\n{}: {}\n{}: {}\n{}: {}",
                                    tr.col_id,
                                    attr.id,
                                    attr.id,
                                    tr.tip_flags,
                                    attr.flags_short,
                                    tr.tip_prefail,
                                    if attr.prefailure {
                                        tr.val_yes
                                    } else {
                                        tr.val_no
                                    },
                                    tr.tip_updated_online,
                                    if attr.updated_online {
                                        tr.val_yes
                                    } else {
                                        tr.val_no
                                    },
                                ));
                        });
                        row.col(|ui| {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(RichText::new(attr.value.to_string()).monospace());
                            });
                        });
                        row.col(|ui| {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(RichText::new(attr.worst.to_string()).monospace());
                            });
                        });
                        row.col(|ui| {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(RichText::new(attr.thresh.to_string()).monospace());
                            });
                        });
                        row.col(|ui| {
                            let raw_text = if cfg.hex_raw {
                                format!("{:012X}", attr.raw_value & 0xFFFF_FFFF_FFFF)
                            } else if !attr.raw_string.is_empty() {
                                attr.raw_string.clone()
                            } else {
                                attr.raw_value.to_string()
                            };
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.add(
                                    egui::Label::new(RichText::new(&raw_text).monospace())
                                        .truncate(),
                                )
                                .on_hover_text(format!(
                                    "{}: {}\n{}: 0x{:012X}\n{}: {}",
                                    tr.tip_decimal,
                                    attr.raw_value,
                                    tr.tip_hex,
                                    attr.raw_value & 0xFFFF_FFFF_FFFF,
                                    tr.tip_smartctl_raw,
                                    attr.raw_string
                                ));
                            });
                        });
                        row.col(|ui| {
                            let localized_note = if attr.prefailure {
                                if let Some(rest) = attr.note.strip_prefix("Pre-fail") {
                                    format!("{}{}", tr.tip_prefail, rest)
                                } else {
                                    attr.note.clone()
                                }
                            } else {
                                attr.note.clone()
                            };
                            let live_note = if (attr.id == 190 || attr.id == 194)
                                && cfg.use_fahrenheit
                                && let Some(c) = attr
                                    .raw_string
                                    .split_whitespace()
                                    .next()
                                    .and_then(|t| t.parse::<i32>().ok())
                            {
                                let f_str = format_temperature(Some(c), true);
                                if attr.prefailure {
                                    format!("{} • {}", tr.tip_prefail, f_str)
                                } else {
                                    f_str
                                }
                            } else if localized_note.is_empty() {
                                "-".to_string()
                            } else {
                                localized_note
                            };
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&live_note)
                                        .size(11.5)
                                        .color(palette.text_secondary),
                                )
                                .truncate(),
                            )
                            .on_hover_text(&live_note);
                        });
                    });
                });
        });
}

fn show_nvme_table(
    ui: &mut Ui,
    disk: &DiskInfo,
    cfg: &AppConfig,
    tr: &Strings,
    palette: &ThemePalette,
    filter: &str,
) {
    let rows: Vec<_> = disk
        .nvme_attributes
        .iter()
        .filter(|a| {
            filter.is_empty()
                || format!("{:02X}", a.id)
                    .to_ascii_lowercase()
                    .contains(filter)
                || a.name.to_ascii_lowercase().contains(filter)
        })
        .collect();

    if rows.is_empty() {
        ui.label(RichText::new(tr.no_matching_attrs).color(palette.text_secondary));
        return;
    }

    let avail_h = ui.available_height().max(90.0);
    ScrollArea::horizontal()
        .id_salt(("nvme_smart_hscroll", &disk.device_node, &disk.device_type))
        .show(ui, |ui| {
            TableBuilder::new(ui)
                .id_salt(("nvme_smart_table", &disk.device_node, &disk.device_type))
                .striped(true)
                .resizable(true)
                .min_scrolled_height(0.0)
                .max_scroll_height(avail_h)
                .auto_shrink([false, false])
                .cell_layout(Layout::left_to_right(Align::Center))
                .column(Column::exact(28.0))
                .column(Column::initial(42.0).at_least(36.0))
                .column(Column::remainder().at_least(200.0))
                .column(Column::initial(155.0).at_least(115.0))
                .column(Column::initial(155.0).at_least(95.0))
                .header(26.0, |mut header| {
                    header.col(|_ui| {});
                    header.col(|ui| {
                        ui.centered_and_justified(|ui| {
                            ui.strong(tr.col_id);
                        });
                    });
                    header.col(|ui| {
                        ui.strong(tr.col_attribute_name);
                    });
                    header.col(|ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.strong(tr.col_raw_values);
                        });
                    });
                    header.col(|ui| {
                        ui.strong(tr.col_decoded);
                    });
                })
                .body(|body| {
                    body.rows(24.0, rows.len(), |mut row| {
                        let attr = rows[row.index()];
                        let (_, _, orb_col) = palette.status_colors(attr.status);
                        let live_decoded = if attr.key == "temperature"
                            || attr.key.starts_with("temperature_sensor_")
                        {
                            let c = (attr.raw_value as i64 - 273) as i32;
                            format_temperature(Some(c), cfg.use_fahrenheit)
                        } else if attr.key == "data_units_read" || attr.key == "data_units_written"
                        {
                            format_data_volume(
                                Some(attr.raw_value * 512_000),
                                cfg.use_gb_instead_of_tb,
                            )
                        } else {
                            attr.decoded_value.clone()
                        };
                        let live_note = if (attr.key == "temperature"
                            || attr.key.starts_with("temperature_sensor_"))
                            && cfg.use_fahrenheit
                        {
                            let c = (attr.raw_value as i64 - 273) as i32;
                            format!(
                                "{} ({} °C / {} K)",
                                format_temperature(Some(c), true),
                                c,
                                attr.raw_value
                            )
                        } else {
                            attr.note.clone()
                        };
                        row.col(|ui| {
                            let (rect, resp) = ui
                                .allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
                            paint_status_orb(ui.painter(), rect.center(), 6.0, orb_col);
                            resp.on_hover_text(tr.health_label(attr.status));
                        });
                        row.col(|ui| {
                            ui.centered_and_justified(|ui| {
                                ui.label(
                                    RichText::new(format!("{:02X}", attr.id))
                                        .monospace()
                                        .strong(),
                                );
                            });
                        });
                        row.col(|ui| {
                            let text = if attr.status != HealthStatus::Good {
                                RichText::new(&attr.name).strong().color(orb_col)
                            } else {
                                RichText::new(&attr.name)
                            };
                            let resp = ui.add(egui::Label::new(text).truncate());
                            if !live_note.is_empty() {
                                resp.on_hover_text(&live_note);
                            }
                        });
                        row.col(|ui| {
                            let raw_text = if cfg.hex_raw {
                                format!("{:016X}", attr.raw_value)
                            } else {
                                attr.raw_value.to_string()
                            };
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.add(
                                    egui::Label::new(RichText::new(&raw_text).monospace())
                                        .truncate(),
                                )
                                .on_hover_text(format!(
                                    "{}: {}\n{}: 0x{:016X}",
                                    tr.tip_decimal, attr.raw_value, tr.tip_hex, attr.raw_value
                                ));
                            });
                        });
                        row.col(|ui| {
                            let resp = ui.add(
                                egui::Label::new(
                                    RichText::new(&live_decoded)
                                        .size(12.0)
                                        .color(palette.text_secondary),
                                )
                                .truncate(),
                            );
                            if !live_note.is_empty() {
                                resp.on_hover_text(&live_note);
                            }
                        });
                    });
                });
        });
}

fn show_scsi_table(
    ui: &mut Ui,
    disk: &DiskInfo,
    tr: &Strings,
    palette: &ThemePalette,
    filter: &str,
) {
    if let Some(gdl) = disk.scsi_grown_defect_list {
        ui.label(
            RichText::new(format!("{}: {}", tr.scsi_grown_defect_list, gdl))
                .strong()
                .color(if gdl > 0 {
                    palette.caution_orb
                } else {
                    palette.text_secondary
                }),
        );
        ui.add_space(4.0);
    }

    let rows: Vec<_> = disk
        .scsi_error_log
        .iter()
        .filter(|r| filter.is_empty() || r.operation.to_ascii_lowercase().contains(filter))
        .collect();

    if rows.is_empty() {
        ui.label(RichText::new(tr.no_matching_attrs).color(palette.text_secondary));
        return;
    }

    let avail_h = ui.available_height().max(90.0);
    ScrollArea::horizontal()
        .id_salt(("scsi_error_hscroll", &disk.device_node, &disk.device_type))
        .show(ui, |ui| {
            TableBuilder::new(ui)
                .id_salt(("scsi_error_table", &disk.device_node, &disk.device_type))
                .striped(true)
                .resizable(true)
                .min_scrolled_height(0.0)
                .max_scroll_height(avail_h)
                .auto_shrink([false, false])
                .cell_layout(Layout::left_to_right(Align::Center))
                .column(Column::exact(28.0))
                .column(Column::remainder().at_least(80.0))
                .column(Column::initial(85.0))
                .column(Column::initial(85.0))
                .column(Column::initial(95.0))
                .column(Column::initial(95.0))
                .column(Column::initial(95.0))
                .column(Column::initial(95.0))
                .column(Column::initial(95.0))
                .header(32.0, |mut header| {
                    header.col(|_ui| {});
                    header.col(|ui| {
                        ui.strong(tr.scsi_col_operation);
                    });
                    header.col(|ui| {
                        ui.strong(tr.scsi_col_ecc_fast);
                    });
                    header.col(|ui| {
                        ui.strong(tr.scsi_col_ecc_delayed);
                    });
                    header.col(|ui| {
                        ui.strong(tr.scsi_col_rereads);
                    });
                    header.col(|ui| {
                        ui.strong(tr.scsi_col_total_corrected);
                    });
                    header.col(|ui| {
                        ui.strong(tr.scsi_col_algo_invocations);
                    });
                    header.col(|ui| {
                        ui.strong(tr.scsi_col_gb_processed);
                    });
                    header.col(|ui| {
                        ui.strong(tr.scsi_col_total_uncorrected);
                    });
                })
                .body(|body| {
                    body.rows(25.0, rows.len(), |mut row| {
                        let r = rows[row.index()];
                        let (_, _, orb_col) = palette.status_colors(r.status);
                        row.col(|ui| {
                            let (rect, resp) = ui
                                .allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
                            paint_status_orb(ui.painter(), rect.center(), 6.0, orb_col);
                            resp.on_hover_text(tr.health_label(r.status));
                        });
                        row.col(|ui| {
                            ui.strong(crate::smartctl::to_title_case(&r.operation));
                        });
                        row.col(|ui| {
                            ui.label(r.ecc_fast.to_string());
                        });
                        row.col(|ui| {
                            ui.label(r.ecc_delayed.to_string());
                        });
                        row.col(|ui| {
                            ui.label(r.rereads_rewrites.to_string());
                        });
                        row.col(|ui| {
                            ui.label(r.total_corrected.to_string());
                        });
                        row.col(|ui| {
                            ui.label(r.algorithm_invocations.to_string());
                        });
                        row.col(|ui| {
                            ui.label(&r.gigabytes_processed);
                        });
                        row.col(|ui| {
                            ui.label(r.total_uncorrected.to_string());
                        });
                    });
                });
        });
}
