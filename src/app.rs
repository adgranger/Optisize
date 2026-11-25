use crate::scan::tree::{Tree, NONE};
use crate::scan::{Msg, Scan};
use crate::ui::{self, drives, side, status, theme::*, toolbar, tree_view::TreeView, treemap::TreemapView, Action, Flags, ScanState};
use crossbeam_channel::{Receiver, TryRecvError};
use eframe::egui::{self, Frame, Margin, Panel};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub struct App {
    tree: Tree,
    path: String,
    filter: String,
    scan: Option<Scan>,
    state: ScanState,
    last_tick: Instant,
    elapsed: Duration,
    errors: u64,
    selected: Option<u32>,
    tv: TreeView,
    tm: TreemapView,
    drives: drives::Shared,
    picker: Option<Receiver<Option<PathBuf>>>,
    cleanup: Vec<(String, u64)>,
    cleanup_total: u64,
    toast: Option<(String, Instant)>,
    ctx: egui::Context,
}

/// Normalise et valide une racine de scan saisie ou choisie.
fn scan_root(input: &str) -> Result<PathBuf, String> {
    let s = input.trim();
    if s.is_empty() {
        return Err("Aucun dossier indiqué".into());
    }
    let b = s.as_bytes();
    // « D: » seul = dossier courant du lecteur D, pas sa racine.
    let p = if b.len() == 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
        PathBuf::from(format!("{}:\\", s[..1].to_ascii_uppercase()))
    } else {
        PathBuf::from(s)
    };
    if p.is_dir() {
        Ok(p)
    } else {
        Err(format!("Dossier introuvable : {}", p.display()))
    }
}

enum Pick {
    Pending,
    Cancelled,
    Chosen(PathBuf),
    Failed,
}

fn poll_pick(rx: &Receiver<Option<PathBuf>>) -> Pick {
    match rx.try_recv() {
        Ok(Some(p)) => Pick::Chosen(p),
        Ok(None) => Pick::Cancelled,
        Err(TryRecvError::Empty) => Pick::Pending,
        Err(TryRecvError::Disconnected) => Pick::Failed,
    }
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, start: Option<String>) -> App {
        ui::theme::apply(&cc.egui_ctx);
        let drives = drives::Shared::default();
        drives::enumerate_async(drives.clone(), cc.egui_ctx.clone());
        let default = std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\".into());
        let mut app = App {
            tree: Tree::new(PathBuf::new()),
            path: start.clone().unwrap_or(default),
            filter: String::new(),
            scan: None,
            state: ScanState::Idle,
            last_tick: Instant::now(),
            elapsed: Duration::ZERO,
            errors: 0,
            selected: None,
            tv: TreeView::new(),
            tm: TreemapView::new(),
            drives,
            picker: None,
            cleanup: Vec::new(),
            cleanup_total: 0,
            toast: None,
            ctx: cc.egui_ctx.clone(),
        };
        if start.is_some() {
            let s = app.path.clone();
            app.open(&s);
        }
        app
    }

    fn toast(&mut self, s: &str) {
        self.toast = Some((s.to_string(), Instant::now()));
    }

    /// Seule entrée vers un nouveau scan : valide, normalise `self.path`, annule le scan en cours.
    fn open(&mut self, input: &str) {
        match scan_root(input) {
            Ok(root) => {
                self.path = root.to_string_lossy().into_owned();
                self.start_scan();
            }
            Err(m) => {
                self.toast(&m);
                drives::enumerate_async(self.drives.clone(), self.ctx.clone());
            }
        }
    }

    fn start_scan(&mut self) {
        let root = PathBuf::from(&self.path);
        self.scan = None; // Drop => annule l'ancien scan
        self.tree = Tree::new(root.clone());
        self.selected = None;
        self.tv.reset();
        self.tm.reset();
        self.scan = Some(Scan::start(root));
        self.state = ScanState::Scanning;
        self.elapsed = Duration::ZERO;
        self.last_tick = Instant::now();
        self.errors = 0;
        self.cleanup.clear();
        self.cleanup_total = 0;
    }

    /// Reçoit les lots des workers (budget de temps par frame pour ne jamais bloquer l'UI).
    fn pump(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        if self.state == ScanState::Scanning {
            self.elapsed += now - self.last_tick;
        }
        self.last_tick = now;

        if let Some(rx) = &self.picker {
            match poll_pick(rx) {
                Pick::Pending => {}
                Pick::Cancelled => self.picker = None,
                Pick::Chosen(p) => {
                    self.picker = None;
                    self.open(&p.to_string_lossy());
                }
                Pick::Failed => {
                    self.picker = None;
                    self.toast("Impossible d'ouvrir le sélecteur de dossier");
                }
            }
        }
        let Some(scan) = &self.scan else { return };
        self.errors = scan.errors();
        let mut done = false;
        let mut more = false;
        loop {
            if now.elapsed() > Duration::from_millis(8) {
                more = true;
                break;
            }
            match scan.rx.try_recv() {
                Ok(Msg::Batch(b)) => self.tree.ingest(b.parent, b.first, &b.entries),
                Ok(Msg::Done) => {
                    done = true;
                    break;
                }
                Err(_) => break,
            }
        }
        if done {
            self.scan = None;
            self.state = ScanState::Done;
            drives::enumerate_async(self.drives.clone(), ctx.clone());
            self.compute_cleanup();
            if self.selected.is_none() {
                self.selected = Some(0);
            }
        } else if more {
            ctx.request_repaint();
        } else if self.state == ScanState::Scanning {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
    }

    /// Stub de « Nettoyage suggéré » : repère les dossiers temp/cache (rien n'est supprimé).
    fn compute_cleanup(&mut self) {
        const NAMES: [&str; 6] = ["temp", "tmp", "cache", "code cache", "gpucache", "shadercache"];
        let t = &self.tree;
        let mut inside = vec![false; t.nodes.len()];
        let mut found: Vec<(u32, u64)> = Vec::new();
        for id in 1..t.nodes.len() {
            let n = &t.nodes[id];
            if n.parent == NONE {
                continue;
            }
            if inside[n.parent as usize] {
                inside[id] = true;
            } else if n.is_dir && n.size > 0 && NAMES.iter().any(|x| t.name(id as u32).eq_ignore_ascii_case(x)) {
                inside[id] = true;
                found.push((id as u32, n.size));
            }
        }
        self.cleanup_total = found.iter().map(|f| f.1).sum();
        found.sort_by_key(|f| std::cmp::Reverse(f.1));
        self.cleanup = found
            .iter()
            .take(3)
            .map(|&(id, s)| {
                let p = t.nodes[id as usize].parent;
                let parent = if p == 0 { String::new() } else { format!("{}\\", t.name(p)) };
                (format!("{parent}{}", t.name(id)), s)
            })
            .collect();
    }

    fn selected_path(&self) -> Option<PathBuf> {
        self.selected.map(|s| self.tree.path(s))
    }

    fn run(&mut self, a: Action, ctx: &egui::Context, frame: &eframe::Frame) {
        match a {
            Action::PickFolder => {
                if self.picker.is_none() {
                    let (tx, rx) = crossbeam_channel::bounded(1);
                    let mut dlg = rfd::FileDialog::new().set_title("Choisir un dossier").set_parent(frame);
                    if std::path::Path::new(&self.path).is_dir() {
                        dlg = dlg.set_directory(&self.path);
                    }
                    std::thread::spawn(move || {
                        tx.send(dlg.pick_folder()).ok();
                    });
                    self.picker = Some(rx);
                }
            }
            Action::Scan | Action::Refresh => {
                if self.state == ScanState::Paused {
                    self.run(Action::TogglePause, ctx, frame);
                } else {
                    self.open(&self.path.clone());
                }
            }
            Action::TogglePause => {
                if let Some(s) = &self.scan {
                    let pause = self.state == ScanState::Scanning;
                    s.set_paused(pause);
                    self.state = if pause { ScanState::Paused } else { ScanState::Scanning };
                    self.last_tick = Instant::now();
                }
            }
            Action::Stop => {
                if self.scan.take().is_some() {
                    self.state = ScanState::Stopped;
                    drives::enumerate_async(self.drives.clone(), ctx.clone());
                    self.compute_cleanup();
                }
            }
            Action::Parent => {
                if let Some(p) = PathBuf::from(self.path.trim()).parent() {
                    self.open(&p.to_string_lossy());
                }
            }
            Action::Clean | Action::Trash => self.toast("Fonction bientôt disponible — aucun fichier n'a été supprimé"),
            Action::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Action::OpenExplorer => {
                if let Some(p) = self.selected_path() {
                    #[cfg(windows)]
                    {
                        use std::os::windows::process::CommandExt;
                        // raw_arg : explorer exige `/select,"chemin"` sans échappement supplémentaire.
                        std::process::Command::new("explorer").raw_arg(format!("/select,\"{}\"", p.display())).spawn().ok();
                    }
                    #[cfg(not(windows))]
                    std::process::Command::new("xdg-open").arg(p).spawn().ok();
                }
            }
            Action::CopyPath => {
                if let Some(p) = self.selected_path() {
                    ctx.copy_text(p.to_string_lossy().into_owned());
                }
            }
            Action::ExpandAll => self.tv.expand_levels(&self.tree, 3),
            Action::CollapseAll => self.tv.collapse_all(),
        }
    }

    fn shortcuts(&self, ctx: &egui::Context) -> Option<Action> {
        let typing = ctx.memory(|m| m.focused().is_some());
        ctx.input(|i| {
            let busy = matches!(self.state, ScanState::Scanning | ScanState::Paused);
            if i.key_pressed(egui::Key::F5) && !busy {
                Some(Action::Scan)
            } else if i.key_pressed(egui::Key::Escape) && busy {
                Some(Action::Stop)
            } else if i.modifiers.command && i.key_pressed(egui::Key::O) {
                Some(Action::PickFolder)
            } else if i.modifiers.command && i.key_pressed(egui::Key::R) && !busy {
                Some(Action::Refresh)
            } else if i.modifiers.alt && i.key_pressed(egui::Key::ArrowUp) && !busy {
                Some(Action::Parent)
            } else if i.key_pressed(egui::Key::Space) && busy && !typing {
                Some(Action::TogglePause)
            } else if i.key_pressed(egui::Key::Enter) && !typing && self.selected.is_some() {
                Some(Action::OpenExplorer)
            } else {
                None
            }
        })
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.pump(&ctx);
        if self.picker.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        let flags = Flags { state: self.state, has_selection: self.selected.is_some() };
        let mut action = self.shortcuts(&ctx);
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let scanning = self.state == ScanState::Scanning;

        Panel::top("menu").frame(Frame::new().fill(BAR).inner_margin(Margin::symmetric(12, 4))).show(ui, |ui| {
            action = ui::menu::show(ui, flags).or(action);
        });
        Panel::top("toolbar").frame(Frame::new().fill(PANEL).inner_margin(Margin::symmetric(16, 10))).show(ui, |ui| {
            action = toolbar::show(ui, flags, &mut self.path, &mut self.filter).or(action);
        });
        if self.toast.as_ref().is_some_and(|t| t.1.elapsed() > Duration::from_secs(5)) {
            self.toast = None;
        }
        Panel::bottom("status").frame(Frame::new().fill(PANEL).inner_margin(Margin::symmetric(16, 8))).show(ui, |ui| {
            status::show(
                ui,
                &status::Info {
                    state: self.state,
                    files: self.tree.nodes[0].files as u64,
                    dirs: self.tree.nodes[0].dirs as u64,
                    elapsed: self.elapsed,
                    errors: self.errors,
                    mem: self.tree.mem_bytes(),
                    toast: self.toast.as_ref().map(|t| t.0.as_str()),
                },
            );
        });

        let mut clicked = None;
        Panel::right("right")
            .default_size(480.0)
            .size_range(360.0..=900.0)
            .resizable(true)
            .frame(Frame::new().fill(PANEL2).inner_margin(Margin::same(16)))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 14.0;
                    let h = (ui.ctx().content_rect().height() * 0.45).clamp(260.0, 620.0);
                    clicked = self.tm.show(ui, &self.tree, self.selected, scanning, h);
                    ui::types::show(ui, &self.tree);
                    let c = side::Cleanup { scanned: matches!(self.state, ScanState::Done | ScanState::Stopped), total: self.cleanup_total, top: &self.cleanup };
                    action = side::cleanup_card(ui, &c).or(action);
                    let sel = self.selected.map(|s| (self.tree.name(s), self.tree.path(s).to_string_lossy().into_owned()));
                    action = side::selection_panel(ui, sel).or(action);
                });
            });

        let mut picked = None;
        egui::CentralPanel::default().frame(Frame::new().fill(BG)).show(ui, |ui| {
            Frame::new().inner_margin(Margin::symmetric(16, 12)).show(ui, |ui| {
                if let Some(root) = drives::show(ui, &self.drives.lock().unwrap().clone(), &self.path) {
                    picked = Some(root);
                }
            });
            self.tv.show(ui, &self.tree, &mut self.selected, self.filter.trim(), scanning, now);
        });

        if let Some(id) = clicked {
            self.selected = Some(id);
            self.tv.reveal(&self.tree, id);
        }
        if let Some(root) = picked {
            self.open(&root);
        }
        if let Some(a) = action {
            self.run(a, &ctx, frame);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        std::env::temp_dir()
    }

    #[test]
    fn scan_root_trims_spaces() {
        let t = tmp();
        assert_eq!(scan_root(&format!("  {}  ", t.display())), Ok(t));
    }

    #[test]
    fn scan_root_empty_is_error() {
        for s in ["", "   "] {
            assert_eq!(scan_root(s), Err("Aucun dossier indiqué".to_string()));
        }
    }

    #[test]
    fn scan_root_missing_dir_names_path() {
        let p = tmp().join("optisize-absent-F01");
        let e = scan_root(&p.to_string_lossy()).unwrap_err();
        assert!(e.contains(&*p.to_string_lossy()), "{e}");
    }

    #[test]
    fn scan_root_file_is_error() {
        let f = tmp().join("optisize-f01-file.txt");
        std::fs::write(&f, b"x").unwrap();
        assert!(scan_root(&f.to_string_lossy()).is_err());
        std::fs::remove_file(&f).ok();
    }

    #[cfg(windows)]
    #[test]
    fn scan_root_bare_drive_gets_root() {
        assert_eq!(scan_root("c:"), Ok(PathBuf::from("C:\\")));
    }

    #[test]
    fn poll_pick_pending() {
        let (_tx, rx) = crossbeam_channel::bounded::<Option<PathBuf>>(1);
        assert!(matches!(poll_pick(&rx), Pick::Pending));
    }

    #[test]
    fn poll_pick_cancelled() {
        let (tx, rx) = crossbeam_channel::bounded(1);
        tx.send(None).unwrap();
        assert!(matches!(poll_pick(&rx), Pick::Cancelled));
    }

    #[test]
    fn poll_pick_chosen() {
        let (tx, rx) = crossbeam_channel::bounded(1);
        tx.send(Some(PathBuf::from("x"))).unwrap();
        assert!(matches!(poll_pick(&rx), Pick::Chosen(p) if p == std::path::Path::new("x")));
    }

    #[test]
    fn poll_pick_disconnected_is_failed() {
        let (tx, rx) = crossbeam_channel::bounded::<Option<PathBuf>>(1);
        drop(tx);
        assert!(matches!(poll_pick(&rx), Pick::Failed));
    }
}
