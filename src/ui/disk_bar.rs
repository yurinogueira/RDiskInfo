use crate::config::AppConfig;
use crate::i18n::Strings;
use crate::model::{DiskInfo, format_capacity, format_temperature};
use crate::theme::ThemePalette;
use crate::ui::artwork::paint_status_orb;
use egui::{Align2, CornerRadius, FontId, Pos2, Rect, ScrollArea, Sense, Stroke, Ui, Vec2};

pub fn show_disk_switcher(
    ui: &mut Ui,
    disks: &[DiskInfo],
    selected_idx: &mut usize,
    cfg: &AppConfig,
    tr: &Strings,
    palette: &ThemePalette,
) {
    if disks.is_empty() {
        return;
    }

    ScrollArea::horizontal()
        .id_salt("disk_switcher_scroll")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                for (idx, disk) in disks.iter().enumerate() {
                    let is_selected = idx == *selected_idx;
                    let health_text = tr.health_label(disk.health_status);
                    let temp_text =
                        format_temperature(disk.temperature.current_celsius, cfg.use_fahrenheit);
                    let duplicate_node = disks
                        .iter()
                        .filter(|other| other.device_node == disk.device_node)
                        .count()
                        > 1;
                    let base_node = if duplicate_node && !disk.device_type.is_empty() {
                        format!("{} [{}]", disk.device_node, disk.device_type)
                    } else {
                        disk.device_node.clone()
                    };
                    let short_node = disk
                        .short_mount_label()
                        .unwrap_or_else(|| base_node.clone());
                    let dev_line = if disk.short_mount_label().is_some() {
                        format!("{} ({})", base_node, short_node)
                    } else {
                        base_node
                    };

                    let (_, _, orb_col) = palette.status_colors(disk.health_status);
                    let text_width =
                        health_text.len().max(temp_text.len()).max(dev_line.len()) as f32 * 7.0;
                    let card_w = (text_width + 38.0).clamp(112.0, 210.0);
                    let card_h = 58.0;

                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::new(card_w, card_h), Sense::click());

                    let bg = if is_selected {
                        palette.card_selected_bg
                    } else if response.hovered() {
                        palette.card_selected_bg.linear_multiply(0.75)
                    } else {
                        palette.card_bg
                    };

                    let stroke = if is_selected {
                        Stroke::new(1.8, palette.accent)
                    } else {
                        Stroke::new(1.0, palette.border)
                    };

                    let painter = ui.painter();
                    painter.rect(
                        rect,
                        CornerRadius::same(6),
                        bg,
                        stroke,
                        egui::StrokeKind::Inside,
                    );

                    // Left colored status bar (QDiskInfo signature!)
                    let bar_rect = Rect::from_min_size(
                        rect.left_top() + Vec2::new(6.0, 8.0),
                        Vec2::new(5.0, card_h - 16.0),
                    );
                    painter.rect_filled(bar_rect, CornerRadius::same(2), orb_col);

                    // Top-right glossy status orb (CrystalDiskInfo signature!)
                    paint_status_orb(
                        painter,
                        Pos2::new(rect.right() - 12.0, rect.top() + 13.0),
                        5.5,
                        orb_col,
                    );

                    let x_text = rect.left() + 17.0;
                    painter.text(
                        Pos2::new(x_text, rect.top() + 9.0),
                        Align2::LEFT_TOP,
                        health_text,
                        FontId::proportional(12.5),
                        palette.text_primary,
                    );
                    painter.text(
                        Pos2::new(x_text, rect.top() + 24.5),
                        Align2::LEFT_TOP,
                        temp_text,
                        FontId::proportional(12.0),
                        palette.text_secondary,
                    );
                    painter.text(
                        Pos2::new(x_text, rect.top() + 40.0),
                        Align2::LEFT_TOP,
                        truncate_middle(&dev_line, 23),
                        FontId::proportional(11.0),
                        palette.text_secondary,
                    );

                    if is_selected {
                        let underline = Rect::from_min_size(
                            Pos2::new(rect.left() + 6.0, rect.bottom() - 3.0),
                            Vec2::new(rect.width() - 12.0, 2.5),
                        );
                        painter.rect_filled(underline, CornerRadius::same(1), palette.accent);
                    }

                    let cap = format_capacity(disk.capacity_bytes, cfg.use_gb_instead_of_tb);
                    let tip = format!(
                        "{} {} : {} : {}",
                        tr.menu_disk,
                        idx + 1,
                        disk.display_title(),
                        cap
                    );
                    if response.on_hover_text(tip).clicked() {
                        *selected_idx = idx;
                    }
                }
            });
        });
}

pub fn show_navigation_header(
    ui: &mut Ui,
    disks: &[DiskInfo],
    selected_idx: &mut usize,
    cfg: &AppConfig,
    palette: &ThemePalette,
) {
    let total = disks.len();
    if total == 0 {
        return;
    }
    let Some(disk) = disks.get(*selected_idx) else {
        return;
    };
    let can_prev = *selected_idx > 0 || (cfg.cyclic_navigation && total > 1);
    let can_next = *selected_idx + 1 < total || (cfg.cyclic_navigation && total > 1);

    let cap = format_capacity(disk.capacity_bytes, cfg.use_gb_instead_of_tb);
    let title = format!("{} : {}", disk.display_title(), cap);
    let header_h = if disk.model_family.is_some() {
        38.0
    } else {
        30.0
    };

    ui.horizontal(|ui| {
        let prev_btn = ui.add_enabled(
            can_prev,
            egui::Button::new("  ◀  ").min_size(Vec2::new(34.0, 30.0)),
        );
        if prev_btn.clicked() && total > 1 {
            *selected_idx = if *selected_idx == 0 {
                total - 1
            } else {
                *selected_idx - 1
            };
        }

        let avail = (ui.available_width() - 42.0).max(120.0);
        ui.allocate_ui_with_layout(
            Vec2::new(avail, header_h),
            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
            |ui| {
                ui.vertical_centered(|ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&title)
                                .size(17.5)
                                .strong()
                                .color(palette.text_primary),
                        )
                        .truncate(),
                    )
                    .on_hover_text(&title);
                    if let Some(fam) = &disk.model_family {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(fam)
                                    .size(11.0)
                                    .color(palette.text_secondary),
                            )
                            .truncate(),
                        );
                    }
                });
            },
        );

        let next_btn = ui.add_enabled(
            can_next,
            egui::Button::new("  ▶  ").min_size(Vec2::new(34.0, 30.0)),
        );
        if next_btn.clicked() && total > 1 {
            *selected_idx = (*selected_idx + 1) % total;
        }
    });
}

fn truncate_middle(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        return s.to_string();
    }
    let head = max_chars.saturating_sub(3) / 2;
    let tail = max_chars.saturating_sub(3).saturating_sub(head);
    let start: String = s.chars().take(head).collect();
    let end: String = s.chars().skip(count.saturating_sub(tail)).collect();
    format!("{}...{}", start, end)
}
