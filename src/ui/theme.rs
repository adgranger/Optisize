use crate::scan::tree::Kind;
use eframe::egui::{self, Color32, FontData, FontFamily, FontId, FontInsert, FontPriority, InsertFontFamily, TextStyle};

pub const BG: Color32 = Color32::from_rgb(0x0f, 0x12, 0x16);
pub const PANEL: Color32 = Color32::from_rgb(0x17, 0x1b, 0x21);
pub const PANEL2: Color32 = Color32::from_rgb(0x12, 0x16, 0x1b);
pub const BAR: Color32 = Color32::from_rgb(0x13, 0x18, 0x20);
pub const BORDER: Color32 = Color32::from_rgb(0x26, 0x2c, 0x36);
pub const BORDER2: Color32 = Color32::from_rgb(0x32, 0x39, 0x46);
pub const TEXT: Color32 = Color32::from_rgb(0xe6, 0xe9, 0xef);
pub const MUTED: Color32 = Color32::from_rgb(0x9a, 0xa4, 0xb2);
pub const ACCENT: Color32 = Color32::from_rgb(0x4e, 0xa1, 0xff);
pub const SEL: Color32 = Color32::from_rgb(0x1f, 0x35, 0x50);
pub const GREEN: Color32 = Color32::from_rgb(0x6f, 0xd0, 0x8c);
pub const YELLOW: Color32 = Color32::from_rgb(0xe8, 0xc9, 0x64);
pub const ORANGE: Color32 = Color32::from_rgb(0xf2, 0xa5, 0x6b);
pub const TRACK: Color32 = Color32::from_rgb(0x23, 0x29, 0x32);
pub const FRAMES: [Color32; 4] = [
    Color32::from_rgb(0x16, 0x20, 0x2e),
    Color32::from_rgb(0x1c, 0x28, 0x38),
    Color32::from_rgb(0x22, 0x30, 0x45),
    Color32::from_rgb(0x28, 0x38, 0x50),
];
pub const FRAME_BORDER: Color32 = Color32::from_rgb(0x34, 0x41, 0x5a);

pub fn kind_color(k: Kind) -> Color32 {
    match k {
        Kind::Video => ORANGE,
        Kind::Audio => Color32::from_rgb(0xe8, 0x8f, 0xc0),
        Kind::Image => Color32::from_rgb(0x5f, 0xd0, 0xc0),
        Kind::Archive => Color32::from_rgb(0xb7, 0x9c, 0xf2),
        Kind::Document => Color32::from_rgb(0x7d, 0xb7, 0xff),
        Kind::Code => YELLOW,
        Kind::Exec => Color32::from_rgb(0x8f, 0xd6, 0x8f),
        Kind::Other => Color32::from_rgb(0x8a, 0x94, 0xa3),
    }
}

pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

/// Nom de la police dans les définitions egui (clé de `font_data`).
pub const MONO_FONT: &str = "JetBrainsMonoNL-Regular";
/// JetBrains Mono NL v2.304, SIL OFL 1.1 (assets/fonts/JetBrainsMono-OFL.txt).
const MONO_TTF: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMonoNL-Regular.ttf");

/// Place JetBrains Mono en tête de la famille Monospace, polices par défaut gardées en repli.
fn install_fonts(ctx: &egui::Context) {
    ctx.add_font(FontInsert::new(
        MONO_FONT,
        FontData::from_static(MONO_TTF),
        vec![InsertFontFamily { family: FontFamily::Monospace, priority: FontPriority::Highest }],
    ));
}

pub fn apply(ctx: &egui::Context) {
    install_fonts(ctx);
    let mut style = (*ctx.global_style()).clone();
    let v = &mut style.visuals;
    *v = egui::Visuals::dark();
    v.panel_fill = PANEL2;
    v.window_fill = Color32::from_rgb(0x1b, 0x20, 0x28);
    v.extreme_bg_color = BG;
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = SEL;
    v.selection.stroke = egui::Stroke::new(1.0, ACCENT);
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, BORDER);
    v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, BORDER2);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x2c, 0x34, 0x41);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(0x2c, 0x34, 0x41);
    for (ts, sz) in [(TextStyle::Body, 13.0), (TextStyle::Button, 13.0), (TextStyle::Small, 11.5)] {
        style.text_styles.insert(ts, FontId::proportional(sz));
    }
    style.text_styles.insert(TextStyle::Monospace, mono(12.0));
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    ctx.set_global_style(style);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> egui::Context {
        let ctx = egui::Context::default();
        apply(&ctx);
        let mut out = ctx.run_ui(Default::default(), |_| {});
        out.textures_delta.clear(); // sinon epaint panique au drop
        ctx
    }

    #[test]
    fn mono_font_bytes_are_truetype() {
        assert_eq!(MONO_TTF.len(), 208_576);
        assert_eq!(MONO_TTF[..4], [0x00, 0x01, 0x00, 0x00]);
    }

    #[test]
    fn jetbrains_mono_heads_monospace() {
        let ctx = ctx();
        ctx.fonts(|f| assert_eq!(f.definitions().families[&FontFamily::Monospace][0], MONO_FONT));
    }

    #[test]
    fn default_fonts_kept_as_fallback() {
        let ctx = ctx();
        ctx.fonts(|f| {
            let fam = &f.definitions().families;
            assert!(fam[&FontFamily::Monospace][1..].iter().any(|n| n == "Hack"));
            assert_ne!(fam[&FontFamily::Proportional][0], MONO_FONT);
        });
    }

    #[test]
    fn mono_font_digits_same_width() {
        let ctx = ctx();
        let w = |s: &str| ctx.fonts_mut(|f| f.layout_no_wrap(s.to_owned(), mono(12.0), Color32::WHITE).size().x);
        assert!((w("1 111,1 Go") - w("8 888,8 Go")).abs() < 0.01);
    }
}
