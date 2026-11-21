use crate::theme::ThemePalette;
use egui::{
    Color32, CornerRadius, Painter, Pos2, Rect, Shape, Stroke, TextureHandle, Vec2,
    epaint::{Mesh, Vertex},
};

pub fn paint_window_background(
    painter: &Painter,
    rect: Rect,
    palette: &ThemePalette,
    custom_bg: Option<&TextureHandle>,
    show_artwork: bool,
) {
    // 1. Vertical gradient base
    let mut mesh = Mesh::default();
    let idx = mesh.vertices.len() as u32;
    mesh.vertices.push(Vertex {
        pos: rect.left_top(),
        uv: Pos2::ZERO,
        color: palette.bg_top,
    });
    mesh.vertices.push(Vertex {
        pos: rect.right_top(),
        uv: Pos2::ZERO,
        color: palette.bg_top,
    });
    mesh.vertices.push(Vertex {
        pos: rect.right_bottom(),
        uv: Pos2::ZERO,
        color: palette.bg_bottom,
    });
    mesh.vertices.push(Vertex {
        pos: rect.left_bottom(),
        uv: Pos2::ZERO,
        color: palette.bg_bottom,
    });
    mesh.add_triangle(idx, idx + 1, idx + 2);
    mesh.add_triangle(idx, idx + 2, idx + 3);
    painter.add(Shape::mesh(mesh));

    // 2. Optional user-chosen wallpaper image (aspect-fill)
    if let Some(tex) = custom_bg {
        let tex_size = tex.size_vec2();
        if tex_size.x > 0.0 && tex_size.y > 0.0 {
            let rect_aspect = rect.width() / rect.height().max(1.0);
            let tex_aspect = tex_size.x / tex_size.y;
            let (u_min, u_max, v_min, v_max) = if tex_aspect > rect_aspect {
                let vis = rect_aspect / tex_aspect;
                let pad = (1.0 - vis) * 0.5;
                (pad, 1.0 - pad, 0.0, 1.0)
            } else {
                let vis = tex_aspect / rect_aspect;
                let pad = (1.0 - vis) * 0.5;
                (0.0, 1.0, pad, 1.0 - pad)
            };
            let uv = Rect::from_min_max(Pos2::new(u_min, v_min), Pos2::new(u_max, v_max));
            let tint = if palette.is_dark {
                Color32::from_rgba_unmultiplied(210, 210, 220, 155)
            } else {
                Color32::from_rgba_unmultiplied(255, 255, 255, 175)
            };
            painter.image(tex.id(), rect, uv, tint);
        }
    }

    // 3. Stylized Sakura/Aoi theme artwork if enabled and no custom wallpaper overrides
    if show_artwork && palette.is_sakura && custom_bg.is_none() {
        paint_sakura_scene(painter, rect, palette.is_dark);
    }
}

fn paint_sakura_scene(painter: &Painter, rect: Rect, is_dark: bool) {
    // Soft diagonal glow band
    let petal_light = if is_dark {
        Color32::from_rgba_unmultiplied(255, 166, 210, 42)
    } else {
        Color32::from_rgba_unmultiplied(255, 172, 210, 88)
    };
    let petal_mid = if is_dark {
        Color32::from_rgba_unmultiplied(238, 120, 182, 34)
    } else {
        Color32::from_rgba_unmultiplied(255, 198, 224, 110)
    };
    let branch_col = if is_dark {
        Color32::from_rgba_unmultiplied(130, 84, 114, 70)
    } else {
        Color32::from_rgba_unmultiplied(150, 100, 120, 58)
    };

    // Subtle decorative sakura branch across top-left / top-right
    let b1 = rect.left_top() + Vec2::new(10.0, 72.0);
    let b2 = rect.left_top() + Vec2::new(185.0, 118.0);
    let b3 = rect.left_top() + Vec2::new(340.0, 96.0);
    painter.line_segment([b1, b2], Stroke::new(3.5, branch_col));
    painter.line_segment([b2, b3], Stroke::new(2.2, branch_col));

    let blossom_positions = [
        (0.08, 0.14, 16.0),
        (0.16, 0.21, 13.0),
        (0.24, 0.17, 15.0),
        (0.32, 0.15, 11.0),
        (0.45, 0.12, 10.0),
        (0.62, 0.22, 12.0),
        (0.75, 0.14, 15.0),
        (0.88, 0.18, 18.0),
        (0.93, 0.34, 13.0),
        (0.11, 0.42, 10.0),
        (0.79, 0.48, 12.0),
        (0.86, 0.72, 14.0),
    ];

    for (nx, ny, radius) in blossom_positions {
        let c = Pos2::new(
            rect.left() + rect.width() * nx,
            rect.top() + rect.height() * ny,
        );
        paint_sakura_blossom(painter, c, radius, petal_mid, petal_light);
    }
}

fn paint_sakura_blossom(
    painter: &Painter,
    center: Pos2,
    radius: f32,
    fill: Color32,
    center_tint: Color32,
) {
    for k in 0..5 {
        let angle = (k as f32) * std::f32::consts::TAU / 5.0 - 0.3;
        let offset = Vec2::new(angle.cos(), angle.sin()) * (radius * 0.56);
        painter.circle_filled(center + offset, radius * 0.46, fill);
    }
    painter.circle_filled(center, radius * 0.26, center_tint);
}

/// Draw a CrystalDiskInfo-inspired mascot badge panel in the right side strip (when space allows).
pub fn paint_sakura_mascot_card(painter: &Painter, rect: Rect, is_dark: bool) {
    let sky_top = if is_dark {
        Color32::from_rgba_unmultiplied(46, 56, 98, 180)
    } else {
        Color32::from_rgba_unmultiplied(128, 196, 246, 195)
    };
    let sky_bot = if is_dark {
        Color32::from_rgba_unmultiplied(74, 46, 88, 180)
    } else {
        Color32::from_rgba_unmultiplied(245, 212, 232, 195)
    };

    let mut mesh = Mesh::default();
    let idx = mesh.vertices.len() as u32;
    mesh.vertices.push(Vertex {
        pos: rect.left_top(),
        uv: Pos2::ZERO,
        color: sky_top,
    });
    mesh.vertices.push(Vertex {
        pos: rect.right_top(),
        uv: Pos2::ZERO,
        color: sky_top,
    });
    mesh.vertices.push(Vertex {
        pos: rect.right_bottom(),
        uv: Pos2::ZERO,
        color: sky_bot,
    });
    mesh.vertices.push(Vertex {
        pos: rect.left_bottom(),
        uv: Pos2::ZERO,
        color: sky_bot,
    });
    mesh.add_triangle(idx, idx + 1, idx + 2);
    mesh.add_triangle(idx, idx + 2, idx + 3);
    painter.add(Shape::mesh(mesh));

    // Soft clouds & sakura petals
    let cloud_col = Color32::from_rgba_unmultiplied(255, 255, 255, if is_dark { 35 } else { 145 });
    painter.circle_filled(
        Pos2::new(rect.left() + rect.width() * 0.25, rect.top() + 55.0),
        28.0,
        cloud_col,
    );
    painter.circle_filled(
        Pos2::new(rect.left() + rect.width() * 0.42, rect.top() + 50.0),
        34.0,
        cloud_col,
    );
    painter.circle_filled(
        Pos2::new(rect.left() + rect.width() * 0.72, rect.top() + 85.0),
        24.0,
        cloud_col,
    );

    // Stylized chibi crystal mascot (clean geometric vector character)
    let cx = rect.center().x;
    let cy = rect.top() + rect.height() * 0.44;
    let scale = (rect.width() / 220.0).clamp(0.72, 1.15);

    let hair_col = if is_dark {
        Color32::from_rgb(104, 82, 118)
    } else {
        Color32::from_rgb(112, 92, 96)
    };
    let skin_col = Color32::from_rgb(255, 229, 214);
    let dress_col = Color32::from_rgb(248, 248, 252);
    let ribbon_col = Color32::from_rgb(66, 152, 224);
    let eye_col = Color32::from_rgb(56, 128, 198);

    // Back hair
    painter.circle_filled(Pos2::new(cx, cy + 4.0 * scale), 46.0 * scale, hair_col);
    painter.rect_filled(
        Rect::from_center_size(
            Pos2::new(cx, cy + 46.0 * scale),
            Vec2::new(78.0 * scale, 64.0 * scale),
        ),
        CornerRadius::same((24.0 * scale) as u8),
        hair_col,
    );

    // Outfit / Capelet
    painter.rect_filled(
        Rect::from_center_size(
            Pos2::new(cx, cy + 76.0 * scale),
            Vec2::new(68.0 * scale, 62.0 * scale),
        ),
        CornerRadius::same((16.0 * scale) as u8),
        dress_col,
    );
    // Sailor collar + blue ribbon
    painter.rect_filled(
        Rect::from_center_size(
            Pos2::new(cx, cy + 54.0 * scale),
            Vec2::new(50.0 * scale, 18.0 * scale),
        ),
        CornerRadius::same(6),
        Color32::from_rgb(228, 236, 248),
    );
    painter.circle_filled(Pos2::new(cx, cy + 62.0 * scale), 7.0 * scale, ribbon_col);

    // Beret / Hat
    painter.circle_filled(
        Pos2::new(cx, cy - 30.0 * scale),
        34.0 * scale,
        Color32::from_rgb(245, 246, 250),
    );
    painter.circle_stroke(
        Pos2::new(cx, cy - 30.0 * scale),
        34.0 * scale,
        Stroke::new(1.5, Color32::from_rgb(198, 206, 222)),
    );

    // Face
    painter.circle_filled(Pos2::new(cx, cy + 2.0 * scale), 34.0 * scale, skin_col);
    // Bangs
    painter.circle_filled(
        Pos2::new(cx - 18.0 * scale, cy - 18.0 * scale),
        18.0 * scale,
        hair_col,
    );
    painter.circle_filled(
        Pos2::new(cx + 18.0 * scale, cy - 18.0 * scale),
        18.0 * scale,
        hair_col,
    );
    painter.circle_filled(Pos2::new(cx, cy - 22.0 * scale), 16.0 * scale, hair_col);

    // Eyes & blush & smile
    painter.circle_filled(
        Pos2::new(cx - 13.0 * scale, cy + 3.0 * scale),
        4.6 * scale,
        eye_col,
    );
    painter.circle_filled(
        Pos2::new(cx + 13.0 * scale, cy + 3.0 * scale),
        4.6 * scale,
        eye_col,
    );
    painter.circle_filled(
        Pos2::new(cx - 14.5 * scale, cy + 1.2 * scale),
        1.6 * scale,
        Color32::WHITE,
    );
    painter.circle_filled(
        Pos2::new(cx + 11.5 * scale, cy + 1.2 * scale),
        1.6 * scale,
        Color32::WHITE,
    );
    let blush = Color32::from_rgba_unmultiplied(255, 140, 165, 110);
    painter.circle_filled(
        Pos2::new(cx - 19.0 * scale, cy + 11.0 * scale),
        5.0 * scale,
        blush,
    );
    painter.circle_filled(
        Pos2::new(cx + 19.0 * scale, cy + 11.0 * scale),
        5.0 * scale,
        blush,
    );

    // Sakura hairpin
    paint_sakura_blossom(
        painter,
        Pos2::new(cx + 25.0 * scale, cy - 16.0 * scale),
        7.5 * scale,
        Color32::from_rgb(255, 182, 212),
        Color32::from_rgb(255, 240, 248),
    );

    // Floating petals inside card
    for (dx, dy, r) in [
        (-65.0, -70.0, 7.0),
        (62.0, -55.0, 8.0),
        (-58.0, 95.0, 6.5),
        (56.0, 120.0, 9.0),
    ] {
        paint_sakura_blossom(
            painter,
            Pos2::new(cx + dx * scale, cy + dy * scale),
            r * scale,
            Color32::from_rgba_unmultiplied(255, 198, 224, 175),
            Color32::from_rgba_unmultiplied(255, 240, 248, 200),
        );
    }

    // Subtle Crystal Seal at bottom right
    let badge_rect = Rect::from_min_size(
        Pos2::new(rect.left() + 10.0, rect.bottom() - 32.0),
        Vec2::new((rect.width() - 20.0).max(80.0), 22.0),
    );
    painter.rect_filled(
        badge_rect,
        CornerRadius::same(6),
        Color32::from_rgba_unmultiplied(255, 255, 255, if is_dark { 30 } else { 175 }),
    );
    paint_sakura_blossom(
        painter,
        Pos2::new(badge_rect.left() + 14.0, badge_rect.center().y),
        6.5,
        if is_dark {
            Color32::from_rgb(248, 150, 198)
        } else {
            Color32::from_rgb(236, 118, 172)
        },
        Color32::from_rgb(255, 240, 248),
    );
    painter.text(
        badge_rect.center() + Vec2::new(7.0, 0.0),
        egui::Align2::CENTER_CENTER,
        "RDiskInfo Sakura",
        egui::FontId::proportional(11.5),
        if is_dark {
            Color32::from_rgb(250, 210, 232)
        } else {
            Color32::from_rgb(160, 52, 106)
        },
    );
}

/// Glossy circular orb (CrystalDiskInfo-style) for disk switcher & SMART status dots.
pub fn paint_status_orb(painter: &Painter, center: Pos2, radius: f32, color: Color32) {
    painter.circle_filled(center, radius, color);
    let highlight = Color32::from_rgba_unmultiplied(255, 255, 255, 135);
    painter.circle_filled(
        center + Vec2::new(-radius * 0.28, -radius * 0.28),
        radius * 0.35,
        highlight,
    );
    painter.circle_stroke(
        center,
        radius,
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(0, 0, 0, 60)),
    );
}
