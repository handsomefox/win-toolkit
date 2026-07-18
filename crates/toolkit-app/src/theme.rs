//! The dark indigo application theme. These color constants are the single
//! source of truth: the egui style and every custom surface reference them so
//! the look stays in sync.

use eframe::egui::{self, Color32};

pub(crate) const BACKGROUND: Color32 = Color32::from_rgb(0x0f, 0x11, 0x15);
pub(crate) const SURFACE: Color32 = Color32::from_rgb(0x18, 0x1b, 0x22);
pub(crate) const SURFACE_RAISED: Color32 = Color32::from_rgb(0x1f, 0x23, 0x2c);
pub(crate) const SURFACE_SUNKEN: Color32 = Color32::from_rgb(0x16, 0x19, 0x22);
pub(crate) const BORDER: Color32 = Color32::from_rgb(0x2a, 0x2f, 0x3a);
pub(crate) const HOVER: Color32 = Color32::from_rgb(0x25, 0x2b, 0x36);
pub(crate) const PRESSED: Color32 = Color32::from_rgb(0x2f, 0x36, 0x43);
pub(crate) const ACCENT: Color32 = Color32::from_rgb(0x63, 0x66, 0xf1);
pub(crate) const SELECTION: Color32 = Color32::from_rgba_premultiplied(0x21, 0x22, 0x50, 0x55);
pub(crate) const ROW_ALT: Color32 = Color32::from_rgba_premultiplied(0x0c, 0x0c, 0x0c, 0x0c);
pub(crate) const TEXT: Color32 = Color32::from_rgb(0xe6, 0xe9, 0xef);
pub(crate) const MUTED: Color32 = Color32::from_rgb(0x8b, 0x93, 0xa3);

// Spacing tokens (f32 — for `add_space` and `vec2`).
pub(crate) const SPACE_SM: f32 = 8.0;
pub(crate) const SPACE_MD: f32 = 12.0;

// Corner radii (u8 — feed `CornerRadius::same` / `Frame::corner_radius`).
pub(crate) const RADIUS_SM: u8 = 6;
pub(crate) const RADIUS_MD: u8 = 9;
pub(crate) const RADIUS_LG: u8 = 14;

// Font sizes (f32).
pub(crate) const FONT_SMALL: f32 = 12.0;
pub(crate) const FONT_BODY: f32 = 14.0;
pub(crate) const FONT_HEADING: f32 = 18.0;

/// Installs the Inter fonts and the dark indigo style on the egui context.
pub(crate) fn apply(ctx: &egui::Context) {
    install_fonts(ctx);

    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(style);
}

fn style(style: &mut egui::Style) {
    use egui::{FontFamily, FontId, TextStyle};

    // Font tokens drive the text styles, retiring most per-call `.size()`.
    style.text_styles = [
        (
            TextStyle::Small,
            FontId::new(FONT_SMALL, FontFamily::Proportional),
        ),
        (
            TextStyle::Body,
            FontId::new(FONT_BODY, FontFamily::Proportional),
        ),
        (
            TextStyle::Button,
            FontId::new(FONT_BODY, FontFamily::Proportional),
        ),
        (
            TextStyle::Heading,
            FontId::new(FONT_HEADING, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(13.0, FontFamily::Monospace),
        ),
    ]
    .into();

    let visuals = &mut style.visuals;
    *visuals = egui::Visuals::dark();

    visuals.panel_fill = BACKGROUND;
    visuals.window_fill = SURFACE;
    visuals.window_stroke = egui::Stroke::new(1.0, BORDER);
    visuals.extreme_bg_color = SURFACE_SUNKEN;
    visuals.faint_bg_color = ROW_ALT;
    visuals.selection.bg_fill = SELECTION;
    visuals.selection.stroke = egui::Stroke::new(1.0, ACCENT);
    visuals.hyperlink_color = ACCENT;

    visuals.widgets.noninteractive.bg_fill = SURFACE;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, BORDER);
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, TEXT);
    visuals.widgets.inactive.bg_fill = SURFACE_RAISED;
    visuals.widgets.inactive.weak_bg_fill = SURFACE_RAISED;
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, TEXT);
    visuals.widgets.hovered.bg_fill = HOVER;
    visuals.widgets.hovered.weak_bg_fill = HOVER;
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.5, TEXT);
    visuals.widgets.active.bg_fill = PRESSED;
    visuals.widgets.active.weak_bg_fill = PRESSED;
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.5, TEXT);
    visuals.widgets.open.bg_fill = SURFACE_RAISED;
    visuals.widgets.open.weak_bg_fill = SURFACE_RAISED;

    // One shared radius for every standard widget; custom frames use the same
    // token scale.
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = egui::CornerRadius::same(RADIUS_SM);
    }
    visuals.window_corner_radius = egui::CornerRadius::same(RADIUS_LG);
    visuals.menu_corner_radius = egui::CornerRadius::same(RADIUS_MD);

    style.spacing.item_spacing = egui::vec2(SPACE_SM, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);

    // Labels are UI chrome, not document text.
    style.interaction.selectable_labels = false;
}

// The vendored Inter files have their Private Use Area cmap entries stripped so
// they do not shadow the Phosphor icon glyphs that live in the same range.
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "inter".to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/Inter-Regular.ttf")).into(),
    );
    fonts.font_data.insert(
        "inter-bold".to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/Inter-Bold.ttf")).into(),
    );
    fonts.font_data.insert(
        "phosphor".to_owned(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/Phosphor.ttf")).into(),
    );
    let proportional = fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default();
    proportional.insert(0, "inter".to_owned());
    // Phosphor is a fallback so glyphs render inside proportional text runs.
    proportional.insert(1, "phosphor".to_owned());
    fonts.families.insert(
        egui::FontFamily::Name("bold".into()),
        vec!["inter-bold".to_owned(), "phosphor".to_owned()],
    );
    ctx.set_fonts(fonts);
}

/// The bold Inter family for headings and emphasized values.
pub(crate) fn bold() -> egui::FontFamily {
    egui::FontFamily::Name("bold".into())
}
