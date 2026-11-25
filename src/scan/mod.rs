//! Scan parallèle : un pool de threads dépile des dossiers, lit leur contenu et envoie un
//! lot par dossier à l'UI. Les ids de nœuds sont réservés via un compteur atomique ; le lot du
//! parent est toujours envoyé avant que ses sous-dossiers ne soient mis en file, donc l'UI
//! reçoit toujours un parent avant ses enfants.

pub mod tree;

use crossbeam_channel::{Receiver, Sender};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering::Relaxed};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};
use tree::Entry;

pub struct Batch {
    pub parent: u32,
    pub first: u32,
    pub entries: Vec<Entry>,
}

pub enum Msg {
    Batch(Batch),
    Done,
}

struct Shared {
    cancel: AtomicBool,
    paused: AtomicBool,
    next_id: AtomicU32,
    pending: AtomicUsize,
    alive: AtomicUsize,
    errors: AtomicU64,
    tasks: Sender<(u32, PathBuf)>,
    results: Sender<Msg>,
}

pub struct Scan {
    pub rx: Receiver<Msg>,
    shared: Arc<Shared>,
}

impl Scan {
    pub fn start(root: PathBuf) -> Scan {
        let (task_tx, task_rx) = crossbeam_channel::unbounded();
        let (res_tx, res_rx) = crossbeam_channel::unbounded();
        let n = std::thread::available_parallelism().map_or(4, |n| n.get() * 2).clamp(4, 32);
        let shared = Arc::new(Shared {
            cancel: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            next_id: AtomicU32::new(1),
            pending: AtomicUsize::new(1),
            alive: AtomicUsize::new(n),
            errors: AtomicU64::new(0),
            tasks: task_tx,
            results: res_tx,
        });
        shared.tasks.send((0, root)).ok();
        for _ in 0..n {
            let (sh, rx) = (shared.clone(), task_rx.clone());
            std::thread::spawn(move || worker(&sh, &rx));
        }
        Scan { rx: res_rx, shared }
    }

    pub fn cancel(&self) {
        self.shared.cancel.store(true, Relaxed);
    }

    pub fn set_paused(&self, p: bool) {
        self.shared.paused.store(p, Relaxed);
    }

    /// Dossiers illisibles (droits, etc.) ignorés jusqu'ici.
    pub fn errors(&self) -> u64 {
        self.shared.errors.load(Relaxed)
    }
}

impl Drop for Scan {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn worker(sh: &Shared, tasks: &Receiver<(u32, PathBuf)>) {
    loop {
        if sh.cancel.load(Relaxed) {
            break;
        }
        if sh.paused.load(Relaxed) {
            std::thread::sleep(Duration::from_millis(30));
            continue;
        }
        match tasks.recv_timeout(Duration::from_millis(5)) {
            Ok((id, path)) => {
                scan_dir(sh, id, path);
                sh.pending.fetch_sub(1, Relaxed);
            }
            Err(_) if sh.pending.load(Relaxed) == 0 => break,
            Err(_) => {}
        }
    }
    if sh.alive.fetch_sub(1, Relaxed) == 1 {
        sh.results.send(Msg::Done).ok();
    }
}

fn is_reparse_dir(m: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        m.is_dir() && m.file_attributes() & 0x400 != 0 // jonctions / liens : on ne les suit pas
    }
    #[cfg(not(windows))]
    {
        let _ = m;
        false
    }
}

fn scan_dir(sh: &Shared, id: u32, path: PathBuf) {
    let Ok(rd) = std::fs::read_dir(&path) else {
        sh.errors.fetch_add(1, Relaxed);
        return;
    };
    let mut entries = Vec::new();
    for e in rd.flatten() {
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        let Ok(m) = e.metadata() else {
            sh.errors.fetch_add(1, Relaxed);
            continue;
        };
        if is_reparse_dir(&m) {
            continue;
        }
        let mtime = m
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs() as u32);
        // TODO: taille allouée (clusters) au lieu de la taille logique.
        let is_dir = m.is_dir();
        entries.push(Entry {
            name: e.file_name().to_string_lossy().into_owned(),
            is_dir,
            size: if is_dir { 0 } else { m.len() },
            mtime,
        });
    }
    if entries.is_empty() {
        return;
    }
    let first = sh.next_id.fetch_add(entries.len() as u32, Relaxed);
    let subdirs: Vec<(u32, PathBuf)> = entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.is_dir)
        .map(|(i, e)| (first + i as u32, path.join(&e.name)))
        .collect();
    sh.pending.fetch_add(subdirs.len(), Relaxed);
    sh.results.send(Msg::Batch(Batch { parent: id, first, entries })).ok();
    for t in subdirs {
        sh.tasks.send(t).ok();
    }
}
