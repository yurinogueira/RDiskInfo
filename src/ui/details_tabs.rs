use crate::config::AppConfig;
use crate::i18n::Strings;
use crate::model::{
    DiskInfo, DiskProtocol, PartitionInfo, TempSample, format_capacity, format_scsi_level,
    format_temperature, temp_status,
};
use crate::theme::ThemePalette;
use egui::{CornerRadius, Pos2, ProgressBar, RichText, ScrollArea, Stroke, Ui, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfTestActionRequest {
    StartShort,
    StartExtended,
    StartConveyance,
    Abort,
}

pub fn show_hardware_tab(
    ui: &mut Ui,
    disk: &DiskInfo,
    cfg: &AppConfig,
    tr: &Strings,
    palette: &ThemePalette,
) {
    ScrollArea::vertical()
        .id_salt(("hw_tab_scroll", &disk.device_node, &disk.device_type))
        .show(ui, |ui| {
            // 1. Temperature Sensors (hwmon + NVMe)
            if !disk.temperature.sensors.is_empty() {
                section_card(ui, tr.sec_temp_sensors, palette, |ui| {
                    egui::Grid::new(("sensors_grid", &disk.device_node, &disk.device_type))
                        .num_columns(4)
                        .spacing([24.0, 6.0])
                        .striped(true)
                        .show(ui, |ui| {
                            ui.strong(tr.col_sensor);
                            ui.strong(tr.col_current);
                            ui.strong(tr.col_max_threshold);
                            ui.strong(tr.col_crit_threshold);
                            ui.end_row();
                            for s in &disk.temperature.sensors {
                                ui.label(&s.label);
                                ui.label(format_temperature(
                                    Some(s.celsius.round() as i32),
                                    cfg.use_fahrenheit,
                                ));
                                ui.label(
                                    s.max_celsius
                                        .map(|m| {
                                            format_temperature(
                                                Some(m.round() as i32),
                                                cfg.use_fahrenheit,
                                            )
                                        })
                                        .unwrap_or_else(|| "----".to_string()),
                                );
                                ui.label(
                                    s.crit_celsius
                                        .map(|c| {
                                            format_temperature(
                                                Some(c.round() as i32),
                                                cfg.use_fahrenheit,
                                            )
                                        })
                                        .unwrap_or_else(|| "----".to_string()),
                                );
                                ui.end_row();
                            }
                        });
                });
                ui.add_space(8.0);
            }

            // 2. PCIe / NVMe Controller Details
            if disk.protocol == DiskProtocol::Nvme || disk.sysfs.pci_bdf.is_some() {
                section_card(ui, tr.sec_nvme_pcie, palette, |ui| {
                    egui::Grid::new(("nvme_pci_grid", &disk.device_node, &disk.device_type))
                        .num_columns(4)
                        .spacing([18.0, 5.0])
                        .show(ui, |ui| {
                            kv_pair(
                                ui,
                                tr.lbl_pci_slot,
                                disk.sysfs.pci_bdf.as_deref().unwrap_or("----"),
                                palette,
                            );
                            kv_pair(
                                ui,
                                tr.lbl_pci_vendor_device,
                                &format!(
                                    "{} / {}",
                                    disk.sysfs
                                        .pci_vendor_id
                                        .as_deref()
                                        .or(disk.nvme_pci_vendor_id.map(|_| "NVMe"))
                                        .unwrap_or("----"),
                                    disk.sysfs.pci_device_id.as_deref().unwrap_or("----")
                                ),
                                palette,
                            );
                            ui.end_row();
                            kv_pair(
                                ui,
                                tr.lbl_pci_subsystem,
                                &format!(
                                    "{} / {}",
                                    disk.sysfs.pci_subsystem_vendor.as_deref().unwrap_or("----"),
                                    disk.sysfs.pci_subsystem_device.as_deref().unwrap_or("----")
                                ),
                                palette,
                            );
                            kv_pair(
                                ui,
                                tr.lbl_nvme_vendor_oui,
                                &format!(
                                    "{} / {}",
                                    disk.nvme_pci_vendor_id
                                        .map(|x| format!("0x{:04X}", x))
                                        .unwrap_or_else(|| "----".to_string()),
                                    disk.nvme_ieee_oui
                                        .map(|x| format!("0x{:06X}", x))
                                        .unwrap_or_else(|| "----".to_string())
                                ),
                                palette,
                            );
                            ui.end_row();
                            kv_pair(
                                ui,
                                tr.lbl_current_pcie_link,
                                &crate::smartctl::format_pcie_link(
                                    disk.sysfs.current_link_speed.as_deref(),
                                    disk.sysfs.current_link_width,
                                ),
                                palette,
                            );
                            kv_pair(
                                ui,
                                tr.lbl_max_pcie_link,
                                &crate::smartctl::format_pcie_link(
                                    disk.sysfs.max_link_speed.as_deref(),
                                    disk.sysfs.max_link_width,
                                ),
                                palette,
                            );
                            ui.end_row();
                            kv_pair(
                                ui,
                                tr.lbl_transport_state,
                                &format!(
                                    "{} / {}",
                                    disk.sysfs.nvme_transport.as_deref().unwrap_or("----"),
                                    disk.sysfs.nvme_state.as_deref().unwrap_or("----")
                                ),
                                palette,
                            );
                            kv_pair(
                                ui,
                                tr.lbl_controller_id,
                                &disk
                                    .nvme_controller_id
                                    .map(|x| x.to_string())
                                    .or_else(|| disk.sysfs.nvme_cntlid.clone())
                                    .unwrap_or_else(|| "----".to_string()),
                                palette,
                            );
                            ui.end_row();
                            kv_pair(
                                ui,
                                tr.lbl_queues_sq_size,
                                &format!(
                                    "{} / {}",
                                    disk.sysfs
                                        .nvme_queue_count
                                        .map(|x| x.to_string())
                                        .unwrap_or_else(|| "----".to_string()),
                                    disk.sysfs
                                        .nvme_sqsize
                                        .map(|x| x.to_string())
                                        .unwrap_or_else(|| "----".to_string())
                                ),
                                palette,
                            );
                            kv_pair(
                                ui,
                                tr.lbl_numa_hmb,
                                &format!(
                                    "{} / {}",
                                    disk.sysfs
                                        .numa_node
                                        .map(|x| x.to_string())
                                        .unwrap_or_else(|| "----".to_string()),
                                    disk.sysfs.nvme_hmb.as_deref().unwrap_or("----")
                                ),
                                palette,
                            );
                            ui.end_row();
                            if disk.nvme_total_capacity_bytes.is_some()
                                || disk.nvme_unallocated_capacity_bytes.is_some()
                            {
                                kv_pair(
                                    ui,
                                    tr.lbl_total_nvm_cap,
                                    &crate::model::format_data_volume(
                                        disk.nvme_total_capacity_bytes,
                                        cfg.use_gb_instead_of_tb,
                                    ),
                                    palette,
                                );
                                kv_pair(
                                    ui,
                                    tr.lbl_unallocated_nvm,
                                    &disk
                                        .nvme_unallocated_capacity_bytes
                                        .map(|b| {
                                            if b == 0 {
                                                "0 B".to_string()
                                            } else {
                                                crate::model::format_data_volume(
                                                    Some(b),
                                                    cfg.use_gb_instead_of_tb,
                                                )
                                            }
                                        })
                                        .unwrap_or_else(|| "----".to_string()),
                                    palette,
                                );
                                ui.end_row();
                            }
                            if let Some(nqn) = &disk.sysfs.nvme_subsysnqn {
                                kv_pair(ui, tr.lbl_subsys_nqn, nqn, palette);
                                ui.end_row();
                            }
                        });

                    if !disk.nvme_namespaces.is_empty() {
                        ui.add_space(6.0);
                        ui.strong(tr.lbl_namespaces);
                        for ns in &disk.nvme_namespaces {
                            ui.label(format!(
                                "NS {}: {}={} | {}={} | {}={} | LBA={} B{}{}",
                                ns.id,
                                tr.partition_size,
                                ns.size_bytes
                                    .map(|b| format_capacity(b, cfg.use_gb_instead_of_tb))
                                    .unwrap_or_else(|| "-".to_string()),
                                tr.lbl_ns_cap,
                                ns.capacity_bytes
                                    .map(|b| format_capacity(b, cfg.use_gb_instead_of_tb))
                                    .unwrap_or_else(|| "-".to_string()),
                                tr.lbl_ns_util,
                                ns.utilization_bytes
                                    .map(|b| format_capacity(b, cfg.use_gb_instead_of_tb))
                                    .unwrap_or_else(|| "-".to_string()),
                                ns.formatted_lba_size.unwrap_or(512),
                                ns.eui64
                                    .as_ref()
                                    .map(|e| format!(" | EUI-64={}", e))
                                    .unwrap_or_default(),
                                ns.nguid
                                    .as_ref()
                                    .map(|g| format!(" | NGUID={}", g))
                                    .unwrap_or_default(),
                            ));
                        }
                    }

                    if !disk.nvme_power_states.is_empty() {
                        ui.add_space(6.0);
                        ui.strong(tr.lbl_power_states);
                        for ps in &disk.nvme_power_states {
                            let lat = match (ps.entry_latency_us, ps.exit_latency_us) {
                                (Some(en), Some(ex)) => format!(" (Ent={}µs, Ex={}µs)", en, ex),
                                _ => String::new(),
                            };
                            let active_prefix = if ps.is_active {
                                format!("[{}] ", tr.lbl_active_badge)
                            } else {
                                String::new()
                            };
                            ui.label(
                                RichText::new(format!(
                                    "{}{}{}",
                                    active_prefix, ps.description, lat
                                ))
                                .monospace()
                                .size(11.0)
                                .color(if ps.is_active {
                                    palette.accent
                                } else {
                                    palette.text_secondary
                                }),
                            );
                        }
                    }
                });
                ui.add_space(8.0);
            }

            // 2b. ATA APM / AAM / Security / SCT Capabilities
            if disk.ata_apm.is_some()
                || disk.ata_aam.is_some()
                || disk.ata_security.is_some()
                || disk.ata_sct.is_some()
            {
                section_card(ui, tr.sec_ata_features, palette, |ui| {
                    egui::Grid::new(("ata_feat_grid", &disk.device_node, &disk.device_type))
                        .num_columns(2)
                        .spacing([18.0, 5.0])
                        .show(ui, |ui| {
                            if let Some(apm) = &disk.ata_apm {
                                let text = if apm.enabled {
                                    apm.level
                                        .map(|l| {
                                            format!("{} ({} {})", tr.val_enabled, tr.lbl_level, l)
                                        })
                                        .unwrap_or_else(|| tr.val_enabled.to_string())
                                } else {
                                    tr.val_disabled.to_string()
                                };
                                kv_pair(ui, tr.lbl_ata_apm, &text, palette);
                                ui.end_row();
                            }
                            if let Some(aam) = &disk.ata_aam {
                                let text = if aam.enabled {
                                    match (aam.level, aam.recommended_level) {
                                        (Some(l), Some(r)) => format!(
                                            "{} ({}, {}: {})",
                                            tr.val_enabled, l, tr.lbl_recommended, r
                                        ),
                                        (Some(l), None) => format!("{} ({})", tr.val_enabled, l),
                                        _ => tr.val_enabled.to_string(),
                                    }
                                } else {
                                    tr.val_disabled.to_string()
                                };
                                kv_pair(ui, tr.lbl_ata_aam, &text, palette);
                                ui.end_row();
                            }
                            if let Some(sec) = &disk.ata_security {
                                let mut parts = Vec::new();
                                if let Some(s) = &sec.state_string {
                                    parts.push(s.clone());
                                } else {
                                    parts.push(if sec.enabled {
                                        tr.val_enabled.to_string()
                                    } else {
                                        tr.val_disabled.to_string()
                                    });
                                }
                                parts.push(format!(
                                    "{}={} | {}={} | {}={}",
                                    tr.lbl_locked,
                                    if sec.locked { tr.val_yes } else { tr.val_no },
                                    tr.lbl_frozen,
                                    if sec.frozen { tr.val_yes } else { tr.val_no },
                                    tr.lbl_expired,
                                    if sec.expired { tr.val_yes } else { tr.val_no },
                                ));
                                kv_pair(ui, tr.lbl_ata_security, &parts.join(" • "), palette);
                                ui.end_row();
                                if sec.normal_erase_minutes.is_some()
                                    || sec.extended_erase_minutes.is_some()
                                {
                                    let norm = sec
                                        .normal_erase_minutes
                                        .map(|m| format!("{} {}", m, tr.minutes_short))
                                        .unwrap_or_else(|| "----".to_string());
                                    let ext = sec
                                        .extended_erase_minutes
                                        .map(|m| format!("{} {}", m, tr.minutes_short))
                                        .unwrap_or_else(|| "----".to_string());
                                    kv_pair(
                                        ui,
                                        tr.lbl_erase_time,
                                        &format!("{} / {}", norm, ext),
                                        palette,
                                    );
                                    ui.end_row();
                                }
                            }
                            if let Some(sct) = &disk.ata_sct {
                                let mut caps = Vec::new();
                                if sct.error_recovery_control_supported {
                                    caps.push("ERC");
                                }
                                if sct.feature_control_supported {
                                    caps.push("FeatureCtrl");
                                }
                                if sct.data_table_supported {
                                    caps.push("DataTable");
                                }
                                let cap_str = if caps.is_empty() {
                                    tr.val_supported.to_string()
                                } else {
                                    caps.join(", ")
                                };
                                let state_str = sct
                                    .device_state
                                    .as_deref()
                                    .map(|st| format!("{} ({})", cap_str, st))
                                    .unwrap_or(cap_str);
                                kv_pair(ui, tr.lbl_ata_sct, &state_str, palette);
                                ui.end_row();

                                if !sct.temperature_history.is_empty()
                                    || sct.sampling_period_minutes.is_some()
                                {
                                    let valid_temps: Vec<i32> = sct
                                        .temperature_history
                                        .iter()
                                        .flatten()
                                        .copied()
                                        .filter(|&t| t > 0 && t < 300)
                                        .collect();
                                    let mut hist_parts = Vec::new();
                                    if let (Some(samp), Some(log_i)) =
                                        (sct.sampling_period_minutes, sct.logging_interval_minutes)
                                    {
                                        hist_parts.push(format!(
                                            "{}: {} / {} {}",
                                            tr.lbl_sampling_interval, samp, log_i, tr.minutes_short
                                        ));
                                    }
                                    if !valid_temps.is_empty() {
                                        let min_t = *valid_temps.iter().min().unwrap();
                                        let max_t = *valid_temps.iter().max().unwrap();
                                        hist_parts.push(format!(
                                            "{}: {} ({}..{})",
                                            tr.lbl_samples,
                                            valid_temps.len(),
                                            format_temperature(Some(min_t), cfg.use_fahrenheit),
                                            format_temperature(Some(max_t), cfg.use_fahrenheit),
                                        ));
                                    }
                                    if !hist_parts.is_empty() {
                                        kv_pair(
                                            ui,
                                            tr.lbl_ata_sct_history,
                                            &hist_parts.join(" • "),
                                            palette,
                                        );
                                        ui.end_row();
                                    }
                                }
                            }
                        });
                });
                ui.add_space(8.0);
            }

            // 3. Kernel Block Queue & Device Flags
            section_card(ui, tr.sec_kernel_queue, palette, |ui| {
                egui::Grid::new(("queue_grid", &disk.device_node, &disk.device_type))
                    .num_columns(4)
                    .spacing([18.0, 5.0])
                    .show(ui, |ui| {
                        kv_pair(
                            ui,
                            tr.lbl_scheduler,
                            disk.sysfs.scheduler.as_deref().unwrap_or("----"),
                            palette,
                        );
                        kv_pair(
                            ui,
                            tr.lbl_rotational_type,
                            match disk.sysfs.rotational {
                                Some(true) => tr.val_rotational_hdd,
                                Some(false) => tr.val_non_rotational_ssd,
                                None => "----",
                            },
                            palette,
                        );
                        ui.end_row();
                        let log_bs = disk.sysfs.logical_block_size.filter(|&v| v > 0).unwrap_or(
                            if disk.logical_block_size > 0 {
                                disk.logical_block_size
                            } else {
                                512
                            },
                        );
                        let phys_bs = disk.sysfs.physical_block_size.filter(|&v| v > 0).unwrap_or(
                            if disk.physical_block_size > 0 {
                                disk.physical_block_size
                            } else {
                                log_bs
                            },
                        );
                        let hw_bs = disk
                            .sysfs
                            .hw_sector_size
                            .filter(|&v| v > 0)
                            .unwrap_or(log_bs);
                        kv_pair(
                            ui,
                            tr.lbl_sector_sizes_hw,
                            &format!("{} B / {} B / {} B", log_bs, phys_bs, hw_bs),
                            palette,
                        );
                        kv_pair(
                            ui,
                            tr.lbl_min_opt_io,
                            &format!(
                                "{} B / {} B",
                                disk.sysfs.minimum_io_size.unwrap_or(0),
                                disk.sysfs.optimal_io_size.unwrap_or(0)
                            ),
                            palette,
                        );
                        ui.end_row();
                        kv_pair(
                            ui,
                            tr.lbl_discard_max_gran,
                            &disk
                                .sysfs
                                .discard_max_bytes
                                .map(|b| {
                                    if b == 0 {
                                        format!("{} / 0 B", tr.val_disabled)
                                    } else {
                                        format!(
                                            "{} / {} B",
                                            format_capacity(b, true),
                                            disk.sysfs.discard_granularity.unwrap_or(0)
                                        )
                                    }
                                })
                                .unwrap_or_else(|| "----".to_string()),
                            palette,
                        );
                        kv_pair(
                            ui,
                            tr.lbl_write_cache_fua,
                            &format!(
                                "{} / {}",
                                disk.sysfs.write_cache.as_deref().unwrap_or("----"),
                                disk.sysfs
                                    .fua
                                    .map(|b| if b { tr.val_yes } else { tr.val_no })
                                    .unwrap_or("----")
                            ),
                            palette,
                        );
                        ui.end_row();
                        kv_pair(
                            ui,
                            tr.lbl_queue_depth_nr,
                            &format!(
                                "{} / {}",
                                disk.sysfs
                                    .queue_depth
                                    .map(|x| x.to_string())
                                    .unwrap_or_else(|| "----".to_string()),
                                disk.sysfs
                                    .nr_requests
                                    .map(|x| x.to_string())
                                    .unwrap_or_else(|| "----".to_string())
                            ),
                            palette,
                        );
                        kv_pair(
                            ui,
                            tr.lbl_max_sectors_ra,
                            &format!(
                                "{} KB / {} KB",
                                disk.sysfs.max_sectors_kb.unwrap_or(0),
                                disk.sysfs.read_ahead_kb.unwrap_or(0)
                            ),
                            palette,
                        );
                        ui.end_row();
                        kv_pair(
                            ui,
                            tr.lbl_flags_rm_ro_hp,
                            &format!(
                                "rm={} ro={} hotplug={}",
                                disk.sysfs.removable, disk.sysfs.read_only, disk.sysfs.hotplug
                            ),
                            palette,
                        );
                        kv_pair(
                            ui,
                            tr.lbl_zoned_ncq,
                            &format!(
                                "{} / {}",
                                disk.sysfs.zoned.as_deref().unwrap_or("----"),
                                disk.sysfs
                                    .ncq_prio_supported
                                    .map(|b| if b { tr.val_supported } else { tr.val_no })
                                    .unwrap_or("----")
                            ),
                            palette,
                        );
                        ui.end_row();
                        if disk.sysfs.vendor.is_some()
                            || disk.sysfs.scsi_level.is_some()
                            || disk.sysfs.wwid.is_some()
                        {
                            let vendor_scsi = format!(
                                "{} / {}",
                                disk.sysfs.vendor.as_deref().unwrap_or("----"),
                                disk.sysfs
                                    .scsi_level
                                    .as_deref()
                                    .map(format_scsi_level)
                                    .unwrap_or_else(|| "----".to_string()),
                            );
                            kv_pair(ui, tr.lbl_vendor_scsi_level, &vendor_scsi, palette);
                            kv_pair(
                                ui,
                                tr.lbl_kernel_wwid,
                                disk.sysfs.wwid.as_deref().unwrap_or("----"),
                                palette,
                            );
                            ui.end_row();
                        }
                    });
            });

            // 4. Live Kernel I/O Counters
            if let Some(st) = &disk.sysfs.io_stats {
                ui.add_space(8.0);
                section_card(ui, tr.sec_io_stats, palette, |ui| {
                    let read_bytes = (st.sectors_read as u128) * 512;
                    let write_bytes = (st.sectors_written as u128) * 512;
                    egui::Grid::new(("iostat_grid", &disk.device_node, &disk.device_type))
                        .num_columns(4)
                        .spacing([18.0, 5.0])
                        .show(ui, |ui| {
                            kv_pair(
                                ui,
                                tr.lbl_reads_completed,
                                &format!(
                                    "{} ({}: {}, {})",
                                    st.reads_completed,
                                    tr.lbl_merged,
                                    st.reads_merged,
                                    crate::model::format_data_volume(
                                        Some(read_bytes),
                                        cfg.use_gb_instead_of_tb
                                    )
                                ),
                                palette,
                            );
                            kv_pair(
                                ui,
                                tr.lbl_writes_completed,
                                &format!(
                                    "{} ({}: {}, {})",
                                    st.writes_completed,
                                    tr.lbl_merged,
                                    st.writes_merged,
                                    crate::model::format_data_volume(
                                        Some(write_bytes),
                                        cfg.use_gb_instead_of_tb
                                    )
                                ),
                                palette,
                            );
                            ui.end_row();
                            kv_pair(
                                ui,
                                tr.lbl_read_io_time,
                                &format!("{} ms / {} ms", st.read_time_ms, st.io_time_ms),
                                palette,
                            );
                            kv_pair(
                                ui,
                                tr.lbl_write_weighted_time,
                                &format!("{} ms / {} ms", st.write_time_ms, st.weighted_io_time_ms),
                                palette,
                            );
                            ui.end_row();
                            kv_pair(
                                ui,
                                tr.lbl_inflight_io,
                                &st.io_in_flight.to_string(),
                                palette,
                            );
                            kv_pair(
                                ui,
                                tr.lbl_discards_completed,
                                &format!(
                                    "{} ({} {})",
                                    st.discards_completed,
                                    st.sectors_discarded,
                                    tr.lbl_sectors_unit
                                ),
                                palette,
                            );
                            ui.end_row();
                        });
                });
            }

            // 5. ATA Device Statistics Pages
            if !disk.ata_device_stats.is_empty() {
                ui.add_space(8.0);
                for page in &disk.ata_device_stats {
                    section_card(
                        ui,
                        &format!(
                            "{} — {} (0x{:02X})",
                            tr.sec_ata_dev_stats, page.page_name, page.page_number
                        ),
                        palette,
                        |ui| {
                            egui::Grid::new((
                                "devstat",
                                &disk.device_node,
                                &disk.device_type,
                                page.page_number,
                            ))
                            .num_columns(3)
                            .spacing([18.0, 4.0])
                            .striped(true)
                            .show(ui, |ui| {
                                for entry in &page.entries {
                                    ui.label(&entry.name);
                                    ui.label(RichText::new(&entry.value).monospace());
                                    ui.label(
                                        RichText::new(entry.raw_hex.as_deref().unwrap_or(""))
                                            .monospace()
                                            .size(11.0)
                                            .color(palette.text_secondary),
                                    );
                                    ui.end_row();
                                }
                            });
                        },
                    );
                    ui.add_space(6.0);
                }
            }

            // 6. SATA PHY Event Counters
            if !disk.sata_phy_counters.is_empty() {
                ui.add_space(8.0);
                section_card(ui, tr.sec_phy_counters, palette, |ui| {
                    egui::Grid::new(("phy_grid", &disk.device_node, &disk.device_type))
                        .num_columns(3)
                        .spacing([18.0, 4.0])
                        .striped(true)
                        .show(ui, |ui| {
                            ui.strong(tr.col_id);
                            ui.strong(tr.col_attribute_name);
                            ui.strong(tr.col_raw_values);
                            ui.end_row();
                            for c in &disk.sata_phy_counters {
                                ui.label(RichText::new(format!("0x{:04X}", c.id)).monospace());
                                ui.label(&c.name);
                                ui.label(RichText::new(c.value.to_string()).monospace());
                                ui.end_row();
                            }
                        });
                });
            }
        });
}

pub fn show_partitions_tab(
    ui: &mut Ui,
    disk: &DiskInfo,
    cfg: &AppConfig,
    tr: &Strings,
    palette: &ThemePalette,
) {
    ScrollArea::vertical()
        .id_salt(("partitions_scroll", &disk.device_node, &disk.device_type))
        .show(ui, |ui| {
            if disk.partitions.is_empty() {
                ui.label(RichText::new(tr.no_partitions_msg).color(palette.text_secondary));
                return;
            }
            for part in &disk.partitions {
                render_partition_card(ui, part, cfg, tr, palette, 0);
                ui.add_space(6.0);
            }
        });
}

fn render_partition_card(
    ui: &mut Ui,
    part: &PartitionInfo,
    cfg: &AppConfig,
    tr: &Strings,
    palette: &ThemePalette,
    depth: usize,
) {
    let indent = (depth as f32) * 18.0;
    ui.horizontal(|ui| {
        if indent > 0.0 {
            ui.add_space(indent);
        }
        egui::Frame::new()
            .fill(palette.card_bg)
            .stroke(Stroke::new(1.0, palette.border))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.set_min_width((ui.available_width() - 8.0).max(200.0));
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&part.path).monospace().strong());
                    ui.label(
                        RichText::new(format!(
                            "[{} • {}]",
                            part.part_type,
                            format_capacity(part.size_bytes, cfg.use_gb_instead_of_tb)
                        ))
                        .color(palette.text_secondary),
                    );
                    if let Some(fs) = &part.fstype {
                        egui::Frame::new()
                            .fill(palette.accent.linear_multiply(0.18))
                            .corner_radius(CornerRadius::same(4))
                            .inner_margin(egui::Margin::symmetric(6, 2))
                            .show(ui, |ui| {
                                ui.label(RichText::new(fs).size(11.5).strong());
                            });
                    }
                    if !part.mountpoints.is_empty() {
                        ui.label(
                            RichText::new(format!(
                                "{}: {}",
                                tr.partition_mount,
                                part.mountpoints.join(", ")
                            ))
                            .strong()
                            .color(palette.accent),
                        );
                    }
                });

                let mut meta_parts = Vec::new();
                if let Some(lbl) = &part.label {
                    meta_parts.push(format!("{}: {}", tr.lbl_part_label, lbl));
                }
                if let Some(pl) = &part.partlabel {
                    meta_parts.push(format!("{}: {}", tr.lbl_part_partlabel, pl));
                }
                if let Some(uuid) = &part.uuid {
                    meta_parts.push(format!("UUID: {}", uuid));
                }
                if !meta_parts.is_empty() {
                    ui.label(
                        RichText::new(meta_parts.join("  •  "))
                            .size(11.5)
                            .color(palette.text_secondary),
                    );
                }

                if let (Some(total), Some(used), Some(avail)) =
                    (part.fs_total_bytes, part.fs_used_bytes, part.fs_avail_bytes)
                    && total > 0
                {
                    let frac = (used as f64 / total as f64).clamp(0.0, 1.0) as f32;
                    let text = format!(
                        "{}: {} / {} ({:.1}%)  •  {}: {}",
                        tr.partition_usage,
                        format_capacity(used, cfg.use_gb_instead_of_tb),
                        format_capacity(total, cfg.use_gb_instead_of_tb),
                        frac * 100.0,
                        tr.free_space,
                        format_capacity(avail, cfg.use_gb_instead_of_tb)
                    );
                    ui.add_space(4.0);
                    ui.add(ProgressBar::new(frac).text(text).show_percentage());
                }
            });
    });

    for child in &part.children {
        ui.add_space(4.0);
        render_partition_card(ui, child, cfg, tr, palette, depth + 1);
    }
}

pub fn show_diagnostics_tab(
    ui: &mut Ui,
    disk: &DiskInfo,
    cfg: &AppConfig,
    tr: &Strings,
    palette: &ThemePalette,
) -> Option<SelfTestActionRequest> {
    show_diagnostics_tab_with_history(ui, disk, &[], cfg, tr, palette)
}

pub fn show_diagnostics_tab_with_history(
    ui: &mut Ui,
    disk: &DiskInfo,
    temp_history: &[TempSample],
    cfg: &AppConfig,
    tr: &Strings,
    palette: &ThemePalette,
) -> Option<SelfTestActionRequest> {
    let mut action = None;

    ScrollArea::vertical()
        .id_salt(("diag_scroll", &disk.device_node, &disk.device_type))
        .show(ui, |ui| {
            // Current Self-Test Execution Status / Progress
            if let Some(exec) = &disk.self_test_execution {
                section_card(ui, tr.sec_selftest_status, palette, |ui| {
                    if exec.in_progress {
                        ui.horizontal_wrapped(|ui| {
                            ui.spinner();
                            let detail = match (exec.completed_percent, exec.remaining_percent) {
                                (Some(comp), Some(rem)) => format!(
                                    "{}: {} ({}% {}, {}% {})",
                                    tr.lbl_selftest_in_progress,
                                    exec.status_text,
                                    comp,
                                    tr.lbl_selftest_completed,
                                    rem,
                                    tr.lbl_selftest_remaining
                                ),
                                _ => {
                                    format!("{}: {}", tr.lbl_selftest_in_progress, exec.status_text)
                                }
                            };
                            ui.label(RichText::new(detail).strong().color(palette.caution_orb));
                            if ui.button(tr.abort_self_test).clicked() {
                                action = Some(SelfTestActionRequest::Abort);
                            }
                        });
                        if let Some(comp) = exec.completed_percent {
                            ui.add_space(4.0);
                            ui.add(
                                ProgressBar::new((comp as f32 / 100.0).clamp(0.0, 1.0))
                                    .show_percentage(),
                            );
                        }
                    } else if !exec.status_text.is_empty() {
                        ui.label(
                            RichText::new(&exec.status_text)
                                .size(12.0)
                                .color(palette.text_secondary),
                        );
                    }
                });
                ui.add_space(8.0);
            }

            // Self-Test Controls
            section_card(ui, tr.start_self_test, palette, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let can_ata_st = disk.protocol == DiskProtocol::Ata
                        && !disk.self_test_polling_minutes.is_empty();
                    let can_nvme_st =
                        disk.protocol == DiskProtocol::Nvme && disk.nvme_self_test_supported;

                    if can_ata_st {
                        for (mode, mins) in &disk.self_test_polling_minutes {
                            let label = match mode.as_str() {
                                "short" => {
                                    format!("{} ({} {})", tr.test_short, mins, tr.minutes_short)
                                }
                                "conveyance" => format!(
                                    "{} ({} {})",
                                    tr.test_conveyance, mins, tr.minutes_short
                                ),
                                "extended" => {
                                    format!("{} ({} {})", tr.test_extended, mins, tr.minutes_short)
                                }
                                other => format!("{} ({} {})", other, mins, tr.minutes_short),
                            };
                            if ui.button(label).clicked() {
                                action = match mode.as_str() {
                                    "short" => Some(SelfTestActionRequest::StartShort),
                                    "conveyance" => Some(SelfTestActionRequest::StartConveyance),
                                    _ => Some(SelfTestActionRequest::StartExtended),
                                };
                            }
                        }
                    } else if can_nvme_st {
                        if ui.button(tr.test_short).clicked() {
                            action = Some(SelfTestActionRequest::StartShort);
                        }
                        if ui.button(tr.test_extended).clicked() {
                            action = Some(SelfTestActionRequest::StartExtended);
                        }
                    } else {
                        ui.label(
                            RichText::new(tr.selftest_unsupported_msg)
                                .color(palette.text_secondary),
                        );
                    }

                    if (can_ata_st || can_nvme_st) && ui.button(tr.abort_self_test).clicked() {
                        action = Some(SelfTestActionRequest::Abort);
                    }
                });
            });

            ui.add_space(8.0);

            // Self-Test Log
            section_card(ui, tr.self_test_log, palette, |ui| {
                if disk.self_test_log.is_empty() {
                    ui.label(RichText::new(tr.no_selftest_log_msg).color(palette.text_secondary));
                } else {
                    egui::Grid::new(("st_log_grid", &disk.device_node, &disk.device_type))
                        .num_columns(5)
                        .spacing([16.0, 4.0])
                        .striped(true)
                        .show(ui, |ui| {
                            ui.strong("#");
                            ui.strong(tr.col_type);
                            ui.strong(tr.col_status);
                            ui.strong(tr.power_on_hours);
                            ui.strong("LBA");
                            ui.end_row();
                            for entry in &disk.self_test_log {
                                ui.label(
                                    entry
                                        .num
                                        .map(|n| n.to_string())
                                        .unwrap_or_else(|| "-".into()),
                                );
                                ui.label(&entry.test_type);
                                ui.label(&entry.status);
                                ui.label(
                                    entry
                                        .lifetime_hours
                                        .map(|h| h.to_string())
                                        .unwrap_or_else(|| "-".into()),
                                );
                                ui.label(entry.lba.as_deref().unwrap_or("-"));
                                ui.end_row();
                            }
                        });
                }
            });

            ui.add_space(8.0);

            // Temperature Range & Limits
            section_card(ui, tr.temperature, palette, |ui| {
                egui::Grid::new(("temp_limits_grid", &disk.device_node, &disk.device_type))
                    .num_columns(4)
                    .spacing([18.0, 5.0])
                    .show(ui, |ui| {
                        kv_pair(
                            ui,
                            tr.col_current,
                            &format_temperature(
                                disk.temperature.current_celsius,
                                cfg.use_fahrenheit,
                            ),
                            palette,
                        );
                        kv_pair(
                            ui,
                            tr.lbl_warning_crit,
                            &format!(
                                "{} / {}",
                                format_temperature(
                                    Some(disk.temperature.warning_threshold),
                                    cfg.use_fahrenheit
                                ),
                                format_temperature(
                                    Some(disk.temperature.critical_threshold),
                                    cfg.use_fahrenheit
                                )
                            ),
                            palette,
                        );
                        ui.end_row();
                        kv_pair(
                            ui,
                            tr.lbl_power_cycle_minmax,
                            &format!(
                                "{} / {}",
                                format_temperature(
                                    disk.temperature.power_cycle_min,
                                    cfg.use_fahrenheit
                                ),
                                format_temperature(
                                    disk.temperature.power_cycle_max,
                                    cfg.use_fahrenheit
                                )
                            ),
                            palette,
                        );
                        kv_pair(
                            ui,
                            tr.lbl_lifetime_minmax,
                            &format!(
                                "{} / {}",
                                format_temperature(
                                    disk.temperature.lifetime_min,
                                    cfg.use_fahrenheit
                                ),
                                format_temperature(
                                    disk.temperature.lifetime_max,
                                    cfg.use_fahrenheit
                                )
                            ),
                            palette,
                        );
                        ui.end_row();
                    });
            });

            ui.add_space(8.0);

            // Session Temperature History Chart & Strip
            section_card(ui, tr.sec_temp_history, palette, |ui| {
                render_temp_history_chart(ui, disk, temp_history, cfg, tr, palette);
            });

            ui.add_space(8.0);

            // Error Log
            section_card(
                ui,
                &format!(
                    "{} ({}: {})",
                    tr.sec_error_log,
                    tr.count_unit,
                    disk.error_log_count.unwrap_or(disk.error_logs.len() as u64)
                ),
                palette,
                |ui| {
                    if disk.error_logs.is_empty() {
                        ui.label(RichText::new(tr.no_error_log_msg).color(palette.text_secondary));
                    } else {
                        for e in &disk.error_logs {
                            let hrs = e
                                .lifetime_hours
                                .map(|h| format!("{}h", h))
                                .unwrap_or_else(|| "-".to_string());
                            ui.label(format!(
                                "#{} | {} | {} | {}",
                                e.error_number.unwrap_or(0),
                                hrs,
                                e.state_or_status,
                                e.description
                            ));
                        }
                    }
                },
            );
        });

    action
}

fn render_temp_history_chart(
    ui: &mut Ui,
    disk: &DiskInfo,
    samples: &[TempSample],
    cfg: &AppConfig,
    tr: &Strings,
    palette: &ThemePalette,
) {
    if samples.is_empty() {
        ui.label(RichText::new(tr.temp_history_empty).color(palette.text_secondary));
        return;
    }

    let min_c = samples.iter().map(|s| s.celsius).min().unwrap_or(0);
    let max_c = samples.iter().map(|s| s.celsius).max().unwrap_or(0);
    let sum_c: i64 = samples.iter().map(|s| i64::from(s.celsius)).sum();
    let avg_c = ((sum_c as f64) / (samples.len() as f64)).round() as i32;

    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(format!(
                "{}: {}  •  {}: {} / {} / {}",
                tr.lbl_samples,
                samples.len(),
                tr.lbl_session_minmaxavg,
                format_temperature(Some(min_c), cfg.use_fahrenheit),
                format_temperature(Some(avg_c), cfg.use_fahrenheit),
                format_temperature(Some(max_c), cfg.use_fahrenheit),
            ))
            .size(11.5)
            .color(palette.text_secondary),
        );
    });
    ui.add_space(4.0);

    let warn_c = disk.temperature.warning_threshold;
    let crit_c = disk.temperature.critical_threshold;
    let y_min = (min_c - 5).min(20) as f32;
    let y_max = (max_c + 5).max(warn_c + 5) as f32;
    let y_span = (y_max - y_min).max(10.0);

    let chart_w = ui.available_width().max(200.0);
    let chart_h = 104.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(chart_w, chart_h), egui::Sense::hover());
    let painter = ui.painter();

    painter.rect(
        rect,
        CornerRadius::same(5),
        palette.field_bg,
        Stroke::new(1.0, palette.border),
        egui::StrokeKind::Inside,
    );

    let plot = rect.shrink2(Vec2::new(46.0, 12.0));
    let y_for_temp = |c: f32| -> f32 {
        let norm = ((c - y_min) / y_span).clamp(0.0, 1.0);
        plot.bottom() - norm * plot.height()
    };

    // Y-axis labels (top & bottom)
    painter.text(
        Pos2::new(rect.left() + 6.0, plot.top()),
        egui::Align2::LEFT_TOP,
        format_temperature(Some(y_max.round() as i32), cfg.use_fahrenheit),
        egui::FontId::proportional(10.0),
        palette.text_secondary,
    );
    painter.text(
        Pos2::new(rect.left() + 6.0, plot.bottom()),
        egui::Align2::LEFT_BOTTOM,
        format_temperature(Some(y_min.round() as i32), cfg.use_fahrenheit),
        egui::FontId::proportional(10.0),
        palette.text_secondary,
    );

    // Warning & Critical threshold reference lines
    for (thr, col) in [(warn_c, palette.caution_orb), (crit_c, palette.bad_orb)] {
        let y = y_for_temp(thr as f32);
        if y >= plot.top() && y <= plot.bottom() {
            painter.line_segment(
                [Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)],
                Stroke::new(1.0, col.linear_multiply(0.55)),
            );
        }
    }

    let n = samples.len();
    let points: Vec<Pos2> = samples
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let x = if n == 1 {
                plot.center().x
            } else {
                plot.left() + (i as f32 / (n - 1) as f32) * plot.width()
            };
            Pos2::new(x, y_for_temp(s.celsius as f32))
        })
        .collect();

    if points.len() >= 2 {
        painter.add(egui::Shape::line(
            points.clone(),
            Stroke::new(2.0, palette.accent),
        ));
    }

    for (pt, s) in points.iter().zip(samples.iter()) {
        let mut t_info = disk.temperature.clone();
        t_info.current_celsius = Some(s.celsius);
        let (_, _, orb_col) = palette.status_colors(temp_status(&t_info));
        painter.circle_filled(*pt, 3.8, orb_col);
        painter.circle_stroke(*pt, 3.8, Stroke::new(1.0, palette.border));
    }

    if let Some(hover_pos) = resp.hover_pos()
        && let Some((idx, _)) = points.iter().enumerate().min_by(|(_, a), (_, b)| {
            (a.x - hover_pos.x)
                .abs()
                .partial_cmp(&(b.x - hover_pos.x).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    {
        let s = &samples[idx];
        resp.on_hover_text(format!(
            "{} — {}",
            s.label,
            format_temperature(Some(s.celsius), cfg.use_fahrenheit)
        ));
    }

    if samples.len() == 1 {
        ui.add_space(3.0);
        ui.label(
            RichText::new(tr.temp_history_single_hint)
                .size(11.0)
                .color(palette.text_secondary),
        );
    }

    // Recent samples strip
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        let start = samples.len().saturating_sub(8);
        for s in &samples[start..] {
            egui::Frame::new()
                .fill(palette.field_bg)
                .stroke(Stroke::new(1.0, palette.border))
                .corner_radius(CornerRadius::same(4))
                .inner_margin(egui::Margin::symmetric(6, 2))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(format!(
                            "{}: {}",
                            s.label,
                            format_temperature(Some(s.celsius), cfg.use_fahrenheit)
                        ))
                        .size(10.5)
                        .monospace()
                        .color(palette.text_primary),
                    );
                });
        }
    });
}

pub fn show_raw_tab(
    ui: &mut Ui,
    disk: &DiskInfo,
    tr: &Strings,
    palette: &ThemePalette,
    show_json: &mut bool,
    search_text: &mut String,
) -> bool {
    let mut copied = false;
    let formatted_full = if *show_json {
        if let Some(raw) = &disk.raw_smartctl_json {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(raw) {
                serde_json::to_string_pretty(&val).unwrap_or_else(|_| raw.clone())
            } else {
                raw.clone()
            }
        } else {
            tr.no_raw_json_msg.to_string()
        }
    } else if disk.raw_smartctl_output.is_empty() {
        tr.no_raw_text_msg.to_string()
    } else {
        disk.raw_smartctl_output.join("\n")
    };

    ui.horizontal(|ui| {
        ui.selectable_value(show_json, false, tr.raw_text_btn);
        ui.selectable_value(show_json, true, tr.raw_json_btn);
        ui.separator();
        ui.add(
            egui::TextEdit::singleline(search_text)
                .desired_width(220.0)
                .hint_text(tr.filter_lines_hint),
        );
        if !search_text.is_empty() && ui.small_button("✖").clicked() {
            search_text.clear();
        }
        if ui.button(format!("📋 {}", tr.copy_btn)).clicked() {
            ui.ctx().copy_text(formatted_full.clone());
            copied = true;
        }
    });
    ui.add_space(4.0);

    let q = search_text.trim().to_ascii_lowercase();
    let displayed = if q.is_empty() {
        formatted_full
    } else {
        formatted_full
            .lines()
            .filter(|l| l.to_ascii_lowercase().contains(&q))
            .collect::<Vec<_>>()
            .join("\n")
    };

    egui::Frame::new()
        .fill(palette.field_bg)
        .stroke(Stroke::new(1.0, palette.border))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ScrollArea::both()
                .id_salt((
                    "raw_text_scroll",
                    &disk.device_node,
                    &disk.device_type,
                    *show_json,
                ))
                .show(ui, |ui| {
                    let mut ro_ref: &str = &displayed;
                    ui.add(
                        egui::TextEdit::multiline(&mut ro_ref)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY),
                    );
                });
        });

    copied
}

fn section_card(
    ui: &mut Ui,
    title: &str,
    palette: &ThemePalette,
    add_contents: impl FnOnce(&mut Ui),
) {
    egui::Frame::new()
        .fill(palette.card_bg)
        .stroke(Stroke::new(1.0, palette.border))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_min_width((ui.available_width() - 4.0).max(200.0));
            ui.label(
                RichText::new(title)
                    .size(13.5)
                    .strong()
                    .color(palette.accent),
            );
            ui.add_space(4.0);
            add_contents(ui);
        });
}

fn kv_pair(ui: &mut Ui, k: &str, v: &str, palette: &ThemePalette) {
    ui.label(RichText::new(k).size(12.0).color(palette.text_secondary));
    ui.label(
        RichText::new(v)
            .size(12.0)
            .monospace()
            .color(palette.text_primary),
    );
}
