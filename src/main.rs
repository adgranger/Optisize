#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod bench;
mod fmt;
mod scan;
mod treemap_layout;
mod ui;

use eframe::egui;

fn main() -> eframe::Result {
    #[cfg(windows)]
    quiet_drive_errors();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--bench") {
        #[cfg(windows)]
        attach_console();
        std::process::exit(bench::run(&args[1..]));
    }
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Optisize")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    let start = args.first().cloned();
    eframe::run_native("Optisize", opts, Box::new(|cc| Ok(Box::new(app::App::new(cc, start)))))
}

/// En release (sous-système « windows »), rattache la sortie standard à la console parente.
#[cfg(windows)]
fn attach_console() {
    #[link(name = "kernel32")]
    extern "system" {
        fn AttachConsole(pid: u32) -> i32;
    }
    unsafe {
        AttachConsole(u32::MAX);
    }
}

/// Pas de boîte système « Il n'y a pas de disque dans le lecteur » (SEM_FAILCRITICALERRORS | SEM_NOOPENFILEERRORBOX).
#[cfg(windows)]
fn quiet_drive_errors() {
    #[link(name = "kernel32")]
    extern "system" {
        fn SetErrorMode(mode: u32) -> u32;
    }
    unsafe {
        SetErrorMode(0x0001 | 0x8000);
    }
}
