// E:\shardvault-rs\src\studio\theme.rs
// Palet Warna & Gaya Visual Anti-Slop untuk ShardVault Studio

use eframe::egui::{Color32, Stroke, Style, Visuals};

pub const COLOR_BG_CANVAS: Color32 = Color32::from_rgb(15, 23, 42);       // #0f172a
pub const COLOR_BG_SURFACE: Color32 = Color32::from_rgb(30, 41, 59);      // #1e293b
pub const COLOR_BG_ELEVATED: Color32 = Color32::from_rgb(51, 65, 85);     // #334155
pub const COLOR_BORDER: Color32 = Color32::from_rgb(71, 85, 105);         // #475569

pub const COLOR_TEXT_PRIMARY: Color32 = Color32::from_rgb(248, 250, 252); // #f8fafc
pub const COLOR_TEXT_SECONDARY: Color32 = Color32::from_rgb(148, 163, 184);// #94a3b8
pub const COLOR_TEXT_MUTED: Color32 = Color32::from_rgb(100, 116, 139);   // #64748b

pub const COLOR_HEALTHY: Color32 = Color32::from_rgb(16, 185, 129);       // #10b981 Emerald
pub const COLOR_SYNCING: Color32 = Color32::from_rgb(59, 130, 246);       // #3b82f6 Blue
pub const COLOR_WARNING: Color32 = Color32::from_rgb(245, 158, 11);       // #f59e0b Amber
pub const COLOR_DANGER: Color32 = Color32::from_rgb(239, 68, 68);         // #ef4444 Red
pub const COLOR_PURPLE: Color32 = Color32::from_rgb(139, 92, 246);        // #8b5cf6 Purple

pub fn apply_studio_theme(ctx: &eframe::egui::Context) {
    let mut visuals = Visuals::dark();
    visuals.override_text_color = Some(COLOR_TEXT_PRIMARY);
    visuals.panel_fill = COLOR_BG_CANVAS;
    visuals.window_fill = COLOR_BG_SURFACE;
    visuals.faint_bg_color = COLOR_BG_SURFACE;
    visuals.extreme_bg_color = Color32::from_rgb(10, 15, 30);
    visuals.code_bg_color = Color32::from_rgb(20, 28, 48);

    visuals.widgets.noninteractive.bg_fill = COLOR_BG_SURFACE;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, COLOR_BORDER);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, COLOR_TEXT_SECONDARY);

    visuals.widgets.inactive.bg_fill = COLOR_BG_ELEVATED;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, COLOR_BORDER);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, COLOR_TEXT_PRIMARY);

    visuals.widgets.hovered.bg_fill = Color32::from_rgb(70, 88, 115);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, COLOR_SYNCING);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, COLOR_TEXT_PRIMARY);

    visuals.widgets.active.bg_fill = COLOR_SYNCING;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, COLOR_SYNCING);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);

    ctx.set_visuals(visuals);

    let mut style = Style::default();
    style.spacing.item_spacing = eframe::egui::vec2(8.0, 6.0);
    style.spacing.button_padding = eframe::egui::vec2(10.0, 5.0);
    ctx.set_style(style);

    // Konfigurasi fallback font sistem Windows (Segoe UI + Segoe UI Emoji)
    // agar semua karakter Unicode, simbol, dan emoji terbaca dengan jernih tanpa tanda tanya/kotak rusak
    let mut fonts = eframe::egui::FontDefinitions::default();
    let mut modified = false;

    if let Ok(font_bytes) = std::fs::read("C:\\Windows\\Fonts\\segoeui.ttf") {
        fonts.font_data.insert(
            "segoeui".to_owned(),
            eframe::egui::FontData::from_owned(font_bytes),
        );
        fonts
            .families
            .entry(eframe::egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "segoeui".to_owned());
        modified = true;
    }

    if let Ok(emoji_bytes) = std::fs::read("C:\\Windows\\Fonts\\seguiemj.ttf") {
        fonts.font_data.insert(
            "seguiemj".to_owned(),
            eframe::egui::FontData::from_owned(emoji_bytes),
        );
        fonts
            .families
            .entry(eframe::egui::FontFamily::Proportional)
            .or_default()
            .push("seguiemj".to_owned());
        fonts
            .families
            .entry(eframe::egui::FontFamily::Monospace)
            .or_default()
            .push("seguiemj".to_owned());
        modified = true;
    }

    if modified {
        ctx.set_fonts(fonts);
    }
}
