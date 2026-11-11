use eframe::egui::{Color32, Context, Stroke, ThemePreference, Visuals};

pub const CANVAS: Color32 = Color32::from_rgb(0x11, 0x15, 0x1B);
pub const PANEL: Color32 = Color32::from_rgb(0x1B, 0x21, 0x2B);
pub const OBJECT: Color32 = Color32::from_rgb(0x24, 0x2D, 0x39);
pub const TEXT: Color32 = Color32::from_rgb(0xE7, 0xED, 0xF5);
pub const MUTED: Color32 = Color32::from_rgb(0xAA, 0xB8, 0xCA);
pub const WIRE: Color32 = Color32::from_rgb(0xCB, 0xD5, 0xE1);
pub const OUTLINE: Color32 = Color32::from_rgb(0x8B, 0x9D, 0xB3);
pub const FOCUS: Color32 = Color32::from_rgb(0x66, 0xC2, 0xFF);

pub fn apply(ctx: &Context) {
    let mut visuals = Visuals::dark();
    visuals.panel_fill = PANEL;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = CANVAS;
    visuals.faint_bg_color = OBJECT;
    visuals.code_bg_color = OBJECT;
    visuals.override_text_color = Some(TEXT);
    visuals.weak_text_color = Some(MUTED);
    visuals.hyperlink_color = Color32::from_rgb(0xA7, 0xB9, 0xFF);
    visuals.warn_fg_color = Color32::from_rgb(0xFF, 0xD1, 0x80);
    visuals.error_fg_color = Color32::from_rgb(0xFF, 0x9B, 0x9B);
    visuals.selection.bg_fill = Color32::from_rgb(0x17, 0x3F, 0x5F);
    visuals.selection.stroke = Stroke::new(1.5, FOCUS);
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
    ] {
        widget.bg_fill = OBJECT;
        widget.weak_bg_fill = OBJECT;
        widget.bg_stroke = Stroke::new(1.0, OUTLINE);
        widget.fg_stroke = Stroke::new(1.0, TEXT);
    }
    for widget in [
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.bg_fill = Color32::from_rgb(0x30, 0x3D, 0x4E);
        widget.weak_bg_fill = widget.bg_fill;
        widget.bg_stroke = Stroke::new(1.5, FOCUS);
        widget.fg_stroke = Stroke::new(1.0, TEXT);
    }
    ctx.set_theme(ThemePreference::Dark);
    ctx.set_visuals(visuals);
}
