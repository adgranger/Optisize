#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod fmt;
mod scan;
mod treemap_layout;
mod ui;

use eframe::egui;

/// `optisize --bench <dossier>` : scan sans interface, affiche le débit (console).
fn bench(path: &str) {
    let t0 = std::time::Instant::now();
    let scan = scan::Scan::start(path.into());
    let mut tree = scan::tree::Tree::new(path.into());
    while let Ok(m) = scan.rx.recv() {
        match m {
            scan::Msg::Batch(b) => tree.ingest(b.parent, b.first, &b.entries),
            scan::Msg::Done => break,
        }
    }
    let secs = t0.elapsed().as_secs_f64();
    let root = &tree.nodes[0];
    println!(
        "{} fichiers, {} dossiers, {} en {secs:.2} s => {:.0} fichiers/s ({:.0} entrées/s), arbre {}",
        fmt::thousands(root.files as u64),
        fmt::thousands(root.dirs as u64),
        fmt::size(root.size),
        root.files as f64 / secs,
        (root.files + root.dirs) as f64 / secs,
        fmt::size(tree.mem_bytes() as u64),
    );
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--bench") {
        #[cfg(windows)]
        attach_console();
        bench(args.get(1).map_or(".", |s| s.as_str()));
        return Ok(());
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
