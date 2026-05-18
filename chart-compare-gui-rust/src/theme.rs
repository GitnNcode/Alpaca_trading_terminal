use egui::Color32;

// Bloomberg palette — same colors as the tview / ratatui ports, expressed as
// egui Color32 so they drop straight into widget styling.

pub const BLACK: Color32 = Color32::from_rgb(0, 0, 0);
pub const ORANGE: Color32 = Color32::from_rgb(255, 102, 0);
pub const CYAN: Color32 = Color32::from_rgb(0, 191, 255);
pub const GREEN: Color32 = Color32::from_rgb(0, 255, 65);
pub const RED: Color32 = Color32::from_rgb(255, 49, 49);
pub const WHITE: Color32 = Color32::from_rgb(255, 255, 255);
pub const GRAY: Color32 = Color32::from_rgb(85, 85, 85);
pub const GRAY2: Color32 = Color32::from_rgb(136, 136, 136);
pub const DARK: Color32 = Color32::from_rgb(13, 13, 13);
pub const YELLOW: Color32 = Color32::from_rgb(255, 215, 0);

/// Apply the dark Bloomberg-style theme to an egui context.
pub fn apply(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.override_text_color = Some(WHITE);
    visuals.panel_fill = BLACK;
    visuals.window_fill = DARK;
    visuals.extreme_bg_color = BLACK;
    visuals.faint_bg_color = DARK;
    visuals.widgets.noninteractive.bg_fill = DARK;
    visuals.widgets.noninteractive.fg_stroke.color = WHITE;
    visuals.widgets.inactive.bg_fill = DARK;
    visuals.widgets.inactive.fg_stroke.color = WHITE;
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(40, 40, 40);
    visuals.widgets.active.bg_fill = ORANGE;
    visuals.widgets.active.fg_stroke.color = BLACK;
    visuals.selection.bg_fill = ORANGE;
    visuals.selection.stroke.color = BLACK;
    visuals.hyperlink_color = CYAN;
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    for font_id in style.text_styles.values_mut() {
        font_id.size *= 1.0; // placeholder for future bump
    }
    ctx.set_style(style);
}
