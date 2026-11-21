use crate::config::ThemeKind;
use crate::model::HealthStatus;
use egui::{Color32, CornerRadius, Stroke, Style, Visuals};

#[derive(Debug, Clone)]
pub struct ThemePalette {
    pub bg_top: Color32,
    pub bg_bottom: Color32,
    pub panel_bg: Color32,
    pub card_bg: Color32,
    pub card_selected_bg: Color32,
    pub field_bg: Color32,
    pub table_row_even: Color32,
    pub table_row_odd: Color32,
    pub border: Color32,
    pub accent: Color32,
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub good_bg: Color32,
    pub good_fg: Color32,
    pub good_orb: Color32,
    pub caution_bg: Color32,
    pub caution_fg: Color32,
    pub caution_orb: Color32,
    pub bad_bg: Color32,
    pub bad_fg: Color32,
    pub bad_orb: Color32,
    pub unknown_bg: Color32,
    pub unknown_fg: Color32,
    pub unknown_orb: Color32,
    pub is_dark: bool,
    pub is_sakura: bool,
}

impl ThemePalette {
    pub fn from_kind(kind: ThemeKind, panel_opacity: f32) -> Self {
        let alpha = (panel_opacity.clamp(0.55, 1.0) * 255.0).round() as u8;
        match kind {
            ThemeKind::NativeLight => Self {
                bg_top: Color32::from_rgb(242, 243, 245),
                bg_bottom: Color32::from_rgb(234, 236, 240),
                panel_bg: Color32::from_rgba_unmultiplied(248, 249, 251, alpha),
                card_bg: Color32::from_rgba_unmultiplied(255, 255, 255, alpha),
                card_selected_bg: Color32::from_rgba_unmultiplied(222, 236, 252, alpha),
                field_bg: Color32::from_rgb(255, 255, 255),
                table_row_even: Color32::from_rgba_unmultiplied(255, 255, 255, alpha),
                table_row_odd: Color32::from_rgba_unmultiplied(244, 246, 249, alpha),
                border: Color32::from_rgb(202, 208, 216),
                accent: Color32::from_rgb(38, 120, 210),
                text_primary: Color32::from_rgb(24, 28, 34),
                text_secondary: Color32::from_rgb(88, 96, 108),
                good_bg: Color32::from_rgb(28, 224, 58),
                good_fg: Color32::from_rgb(12, 20, 14),
                good_orb: Color32::from_rgb(24, 205, 52),
                caution_bg: Color32::from_rgb(250, 208, 32),
                caution_fg: Color32::from_rgb(25, 20, 5),
                caution_orb: Color32::from_rgb(242, 188, 15),
                bad_bg: Color32::from_rgb(236, 62, 62),
                bad_fg: Color32::from_rgb(255, 255, 255),
                bad_orb: Color32::from_rgb(228, 48, 48),
                unknown_bg: Color32::from_rgb(162, 168, 176),
                unknown_fg: Color32::from_rgb(20, 22, 26),
                unknown_orb: Color32::from_rgb(138, 146, 156),
                is_dark: false,
                is_sakura: false,
            },
            ThemeKind::ModernDark => Self {
                bg_top: Color32::from_rgb(24, 27, 34),
                bg_bottom: Color32::from_rgb(18, 20, 26),
                panel_bg: Color32::from_rgba_unmultiplied(31, 35, 44, alpha),
                card_bg: Color32::from_rgba_unmultiplied(38, 43, 54, alpha),
                card_selected_bg: Color32::from_rgba_unmultiplied(46, 68, 98, alpha),
                field_bg: Color32::from_rgb(27, 30, 38),
                table_row_even: Color32::from_rgba_unmultiplied(31, 35, 44, alpha),
                table_row_odd: Color32::from_rgba_unmultiplied(37, 42, 52, alpha),
                border: Color32::from_rgb(66, 74, 90),
                accent: Color32::from_rgb(77, 163, 255),
                text_primary: Color32::from_rgb(235, 240, 248),
                text_secondary: Color32::from_rgb(158, 170, 188),
                good_bg: Color32::from_rgb(38, 182, 92),
                good_fg: Color32::from_rgb(255, 255, 255),
                good_orb: Color32::from_rgb(46, 204, 113),
                caution_bg: Color32::from_rgb(235, 178, 32),
                caution_fg: Color32::from_rgb(20, 16, 4),
                caution_orb: Color32::from_rgb(243, 186, 36),
                bad_bg: Color32::from_rgb(218, 56, 64),
                bad_fg: Color32::from_rgb(255, 255, 255),
                bad_orb: Color32::from_rgb(235, 68, 76),
                unknown_bg: Color32::from_rgb(104, 114, 128),
                unknown_fg: Color32::from_rgb(245, 247, 250),
                unknown_orb: Color32::from_rgb(122, 132, 146),
                is_dark: true,
                is_sakura: false,
            },
            ThemeKind::CrystalBlue => Self {
                bg_top: Color32::from_rgb(225, 239, 252),
                bg_bottom: Color32::from_rgb(240, 246, 252),
                panel_bg: Color32::from_rgba_unmultiplied(245, 250, 255, alpha),
                card_bg: Color32::from_rgba_unmultiplied(255, 255, 255, alpha),
                card_selected_bg: Color32::from_rgba_unmultiplied(204, 232, 255, alpha),
                field_bg: Color32::from_rgb(247, 251, 255),
                table_row_even: Color32::from_rgba_unmultiplied(255, 255, 255, alpha),
                table_row_odd: Color32::from_rgba_unmultiplied(238, 246, 254, alpha),
                border: Color32::from_rgb(164, 200, 232),
                accent: Color32::from_rgb(26, 132, 224),
                text_primary: Color32::from_rgb(18, 32, 48),
                text_secondary: Color32::from_rgb(76, 98, 122),
                good_bg: Color32::from_rgb(108, 204, 252),
                good_fg: Color32::from_rgb(10, 24, 38),
                good_orb: Color32::from_rgb(48, 172, 242),
                caution_bg: Color32::from_rgb(252, 212, 68),
                caution_fg: Color32::from_rgb(32, 24, 4),
                caution_orb: Color32::from_rgb(244, 184, 24),
                bad_bg: Color32::from_rgb(242, 82, 82),
                bad_fg: Color32::from_rgb(255, 255, 255),
                bad_orb: Color32::from_rgb(226, 54, 54),
                unknown_bg: Color32::from_rgb(174, 184, 196),
                unknown_fg: Color32::from_rgb(22, 26, 32),
                unknown_orb: Color32::from_rgb(140, 152, 166),
                is_dark: false,
                is_sakura: false,
            },
            ThemeKind::SakuraAoi => Self {
                bg_top: Color32::from_rgb(252, 232, 241),
                bg_bottom: Color32::from_rgb(218, 238, 254),
                panel_bg: Color32::from_rgba_unmultiplied(255, 250, 253, alpha),
                card_bg: Color32::from_rgba_unmultiplied(255, 255, 255, alpha),
                card_selected_bg: Color32::from_rgba_unmultiplied(255, 222, 238, alpha),
                field_bg: Color32::from_rgba_unmultiplied(252, 245, 249, 235),
                table_row_even: Color32::from_rgba_unmultiplied(255, 255, 255, alpha),
                table_row_odd: Color32::from_rgba_unmultiplied(252, 240, 247, alpha),
                border: Color32::from_rgb(232, 182, 206),
                accent: Color32::from_rgb(212, 76, 138),
                text_primary: Color32::from_rgb(38, 24, 38),
                text_secondary: Color32::from_rgb(112, 78, 102),
                good_bg: Color32::from_rgb(122, 208, 252),
                good_fg: Color32::from_rgb(14, 24, 38),
                good_orb: Color32::from_rgb(76, 178, 242),
                caution_bg: Color32::from_rgb(255, 214, 92),
                caution_fg: Color32::from_rgb(38, 24, 4),
                caution_orb: Color32::from_rgb(245, 185, 36),
                bad_bg: Color32::from_rgb(240, 78, 104),
                bad_fg: Color32::from_rgb(255, 255, 255),
                bad_orb: Color32::from_rgb(224, 52, 82),
                unknown_bg: Color32::from_rgb(186, 178, 192),
                unknown_fg: Color32::from_rgb(28, 22, 30),
                unknown_orb: Color32::from_rgb(154, 146, 162),
                is_dark: false,
                is_sakura: true,
            },
            ThemeKind::SakuraNight => Self {
                bg_top: Color32::from_rgb(32, 22, 44),
                bg_bottom: Color32::from_rgb(20, 26, 48),
                panel_bg: Color32::from_rgba_unmultiplied(38, 28, 52, alpha),
                card_bg: Color32::from_rgba_unmultiplied(48, 36, 66, alpha),
                card_selected_bg: Color32::from_rgba_unmultiplied(92, 52, 98, alpha),
                field_bg: Color32::from_rgba_unmultiplied(34, 24, 48, 235),
                table_row_even: Color32::from_rgba_unmultiplied(38, 28, 52, alpha),
                table_row_odd: Color32::from_rgba_unmultiplied(46, 34, 62, alpha),
                border: Color32::from_rgb(112, 74, 128),
                accent: Color32::from_rgb(244, 128, 186),
                text_primary: Color32::from_rgb(248, 238, 248),
                text_secondary: Color32::from_rgb(198, 168, 198),
                good_bg: Color32::from_rgb(64, 186, 228),
                good_fg: Color32::from_rgb(10, 18, 28),
                good_orb: Color32::from_rgb(90, 206, 245),
                caution_bg: Color32::from_rgb(246, 194, 66),
                caution_fg: Color32::from_rgb(24, 16, 4),
                caution_orb: Color32::from_rgb(252, 204, 82),
                bad_bg: Color32::from_rgb(232, 68, 96),
                bad_fg: Color32::from_rgb(255, 255, 255),
                bad_orb: Color32::from_rgb(244, 82, 108),
                unknown_bg: Color32::from_rgb(112, 104, 126),
                unknown_fg: Color32::from_rgb(245, 240, 250),
                unknown_orb: Color32::from_rgb(134, 124, 148),
                is_dark: true,
                is_sakura: true,
            },
            ThemeKind::CyberMidnight => Self {
                bg_top: Color32::from_rgb(11, 16, 24),
                bg_bottom: Color32::from_rgb(8, 12, 18),
                panel_bg: Color32::from_rgba_unmultiplied(16, 24, 36, alpha),
                card_bg: Color32::from_rgba_unmultiplied(20, 31, 46, alpha),
                card_selected_bg: Color32::from_rgba_unmultiplied(18, 66, 86, alpha),
                field_bg: Color32::from_rgb(13, 20, 30),
                table_row_even: Color32::from_rgba_unmultiplied(16, 24, 36, alpha),
                table_row_odd: Color32::from_rgba_unmultiplied(21, 32, 48, alpha),
                border: Color32::from_rgb(42, 92, 118),
                accent: Color32::from_rgb(0, 224, 212),
                text_primary: Color32::from_rgb(228, 246, 252),
                text_secondary: Color32::from_rgb(126, 174, 194),
                good_bg: Color32::from_rgb(0, 212, 152),
                good_fg: Color32::from_rgb(6, 20, 18),
                good_orb: Color32::from_rgb(16, 232, 168),
                caution_bg: Color32::from_rgb(255, 196, 36),
                caution_fg: Color32::from_rgb(22, 16, 2),
                caution_orb: Color32::from_rgb(255, 208, 64),
                bad_bg: Color32::from_rgb(255, 62, 92),
                bad_fg: Color32::from_rgb(255, 255, 255),
                bad_orb: Color32::from_rgb(255, 82, 108),
                unknown_bg: Color32::from_rgb(88, 108, 124),
                unknown_fg: Color32::from_rgb(235, 244, 250),
                unknown_orb: Color32::from_rgb(108, 128, 144),
                is_dark: true,
                is_sakura: false,
            },
        }
    }

    pub fn status_colors(&self, status: HealthStatus) -> (Color32, Color32, Color32) {
        match status {
            HealthStatus::Good => (self.good_bg, self.good_fg, self.good_orb),
            HealthStatus::Caution => (self.caution_bg, self.caution_fg, self.caution_orb),
            HealthStatus::Bad => (self.bad_bg, self.bad_fg, self.bad_orb),
            HealthStatus::Unknown => (self.unknown_bg, self.unknown_fg, self.unknown_orb),
        }
    }

    pub fn apply_to_ctx(&self, ctx: &egui::Context) {
        let mut style = Style::default();
        let mut visuals = if self.is_dark {
            Visuals::dark()
        } else {
            Visuals::light()
        };

        visuals.panel_fill = self.panel_bg;
        visuals.window_fill = self.panel_bg;
        visuals.extreme_bg_color = self.field_bg;
        visuals.faint_bg_color = self.table_row_odd;
        visuals.override_text_color = Some(self.text_primary);
        visuals.selection.bg_fill = self.accent.linear_multiply(0.30);
        visuals.selection.stroke = Stroke::new(1.5, self.accent);

        visuals.widgets.noninteractive.bg_fill = self.panel_bg;
        visuals.widgets.noninteractive.weak_bg_fill = self.card_bg;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, self.border);
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, self.text_primary);

        visuals.widgets.inactive.bg_fill = self.card_bg;
        visuals.widgets.inactive.weak_bg_fill = self.card_bg;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, self.border);
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, self.text_primary);

        visuals.widgets.hovered.bg_fill = self.card_selected_bg;
        visuals.widgets.hovered.weak_bg_fill = self.card_selected_bg;
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.2, self.accent);
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.5, self.text_primary);

        visuals.widgets.active.bg_fill = self.accent;
        visuals.widgets.active.weak_bg_fill = self.accent;
        visuals.widgets.active.bg_stroke = Stroke::new(1.5, self.accent);
        visuals.widgets.active.fg_stroke = Stroke::new(1.5, Color32::WHITE);

        let cr = CornerRadius::same(6);
        visuals.widgets.noninteractive.corner_radius = cr;
        visuals.widgets.inactive.corner_radius = cr;
        visuals.widgets.hovered.corner_radius = cr;
        visuals.widgets.active.corner_radius = cr;
        visuals.window_corner_radius = CornerRadius::same(8);

        style.visuals = visuals;
        style.spacing.item_spacing = egui::vec2(8.0, 5.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        let egui_theme = if self.is_dark {
            egui::Theme::Dark
        } else {
            egui::Theme::Light
        };
        ctx.set_theme(egui_theme);
        ctx.set_style_of(egui_theme, style);
    }
}
