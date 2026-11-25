use super::theme::*;
use crate::fmt;
use eframe::egui::{self, Sense, Ui};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Drive {
    pub name: String,
    pub root: String,
    pub used: u64,
    pub total: u64,
}

pub type Shared = Arc<Mutex<Vec<Drive>>>;

/// Énumère les lecteurs dans un thread (un lecteur réseau déconnecté peut bloquer).
pub fn enumerate_async(out: Shared, ctx: egui::Context) {
    std::thread::spawn(move || {
        *out.lock().unwrap() = enumerate();
        ctx.request_repaint();
    });
}

#[cfg(windows)]
fn enumerate() -> Vec<Drive> {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetLogicalDrives() -> u32;
        fn GetDiskFreeSpaceExW(dir: *const u16, avail: *mut u64, total: *mut u64, free: *mut u64) -> i32;
    }
    let mask = unsafe { GetLogicalDrives() };
    (0..26u8)
        .filter(|i| mask & (1 << i) != 0)
        .filter_map(|i| {
            let letter = (b'A' + i) as char;
            let root = format!("{letter}:\\");
            let wide: Vec<u16> = root.encode_utf16().chain(Some(0)).collect();
            let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
            let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut avail, &mut total, &mut free) };
            (ok != 0 && total > 0).then(|| Drive { name: format!("{letter}:"), root, used: total - free, total })
        })
        .collect()
}

#[cfg(not(windows))]
fn enumerate() -> Vec<Drive> {
    Vec::new()
}

/// Retourne la racine du lecteur cliqué.
pub fn show(ui: &mut Ui, drives: &[Drive], current: &str) -> Option<String> {
    let mut clicked = None;
    ui.horizontal_wrapped(|ui| {
        for d in drives {
            let active = current.to_ascii_uppercase().starts_with(&d.name);
            let pct = d.used as f32 / d.total as f32;
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(230.0, 46.0), Sense::click());
            let p = ui.painter();
            p.rect_filled(rect, 8.0, PANEL);
            let stroke = egui::Stroke::new(1.0, if active { ACCENT } else { BORDER });
            p.rect_stroke(rect, 8.0, stroke, egui::StrokeKind::Inside);
            if resp.hovered() {
                p.rect_filled(rect, 8.0, egui::Color32::from_white_alpha(8));
            }
            p.text(rect.left_top() + egui::vec2(12.0, 9.0), egui::Align2::LEFT_TOP, &d.name, egui::FontId::proportional(13.0), TEXT);
            let used = format!("{} / {}", fmt::size(d.used), fmt::size(d.total));
            p.text(rect.right_top() + egui::vec2(-12.0, 10.0), egui::Align2::RIGHT_TOP, used, mono(11.0), MUTED);
            let bar = egui::Rect::from_min_size(rect.left_bottom() + egui::vec2(12.0, -14.0), egui::vec2(rect.width() - 24.0, 4.0));
            p.rect_filled(bar, 2.0, egui::Color32::from_rgb(0x2a, 0x30, 0x3a));
            let col = if pct > 0.85 { ORANGE } else if active { ACCENT } else { GREEN };
            p.rect_filled(egui::Rect::from_min_size(bar.min, egui::vec2(bar.width() * pct, 4.0)), 2.0, col);
            if resp.on_hover_text(format!("Scanner {}", d.root)).clicked() {
                clicked = Some(d.root.clone());
            }
        }
    });
    clicked
}
