//! Scan parallèle : un pool de threads dépile des dossiers, lit leur contenu et envoie un
//! lot par dossier à l'UI. Les ids de nœuds sont réservés via un compteur atomique ; le lot du
//! parent est toujours envoyé avant que ses sous-dossiers ne soient mis en file, donc l'UI
//! reçoit toujours un parent avant ses enfants.

pub mod tree;
#[cfg(windows)]
mod win;

use crossbeam_channel::{Receiver, Sender};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, UNIX_EPOCH};
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

/// Temps cumulés des workers (nanosecondes), mesurés en permanence (≈ 25 ns par mesure),
/// ajoutés une fois par worker à `Shared::phases`.
#[derive(Clone, Copy, Default, Debug)]
pub struct Phases {
    /// Ouverture du dossier (`CreateFileW`).
    pub open: u64,
    /// Appels `GetFileInformationByHandleEx`.
    pub query: u64,
    /// Décodage du tampon, appels WOF compris.
    pub parse: u64,
    /// Envoi du lot et mise en file des sous-dossiers.
    pub send: u64,
    /// Appels `GetCompressedFileSizeW`.
    pub wof_calls: u64,
    /// Dossiers dont la lecture a été tentée.
    pub dirs_read: u64,
}

impl std::ops::AddAssign for Phases {
    fn add_assign(&mut self, o: Phases) {
        self.open += o.open;
        self.query += o.query;
        self.parse += o.parse;
        self.send += o.send;
        self.wof_calls += o.wof_calls;
        self.dirs_read += o.dirs_read;
    }
}

/// Nanosecondes écoulées depuis `t`.
fn ns(t: Instant) -> u64 {
    t.elapsed().as_nanos() as u64
}

/// Nombre de workers : `OPTISIZE_THREADS` (1..=64, réglage de mesure) sinon cœurs logiques bornés
/// à 4..8 (F05 : au-delà de 8 l'ouverture des dossiers se sérialise dans le noyau, voir docs/bench.md).
fn worker_count(env: Option<&str>, cpus: usize) -> usize {
    env.and_then(|s| s.trim().parse().ok()).filter(|n| (1..=64).contains(n)).unwrap_or(cpus.clamp(4, 8))
}

/// Dossier à lire : id réservé, chemin complet.
type Task = (u32, PathBuf);

struct Shared {
    cancel: AtomicBool,
    paused: AtomicBool,
    next_id: AtomicU32,
    pending: AtomicUsize,
    alive: AtomicUsize,
    errors: AtomicU64,
    /// La racine elle-même n'a pas pu être lue (ACL, dossier protégé).
    root_denied: AtomicBool,
    phases: Mutex<Phases>,
    threads: usize,
    /// Volume NTFS ou ReFS : identifiants de fichier fiables, liens physiques dédoublonnés.
    fs_dedupe: bool,
    /// Taille de cluster du volume (0 si inconnue : ni arrondi ni seuil).
    cluster: u64,
    tasks: Sender<Task>,
    results: Sender<Msg>,
}

pub struct Scan {
    pub rx: Receiver<Msg>,
    shared: Arc<Shared>,
}

impl Scan {
    pub fn start(root: PathBuf) -> Scan {
        let cpus = std::thread::available_parallelism().map_or(2, |n| n.get());
        Scan::start_with(root, worker_count(std::env::var("OPTISIZE_THREADS").ok().as_deref(), cpus))
    }

    pub(crate) fn start_with(root: PathBuf, n: usize) -> Scan {
        // Les workers utilisent des chemins verbatim (préfixe `\\?\`) : il les faut absolus.
        let root = std::path::absolute(&root).unwrap_or(root);
        let (fs, cluster) = volume_info(&root);
        let (task_tx, task_rx) = crossbeam_channel::unbounded();
        let (res_tx, res_rx) = crossbeam_channel::unbounded();
        let shared = Arc::new(Shared {
            cancel: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            next_id: AtomicU32::new(1),
            pending: AtomicUsize::new(1),
            alive: AtomicUsize::new(n),
            errors: AtomicU64::new(0),
            root_denied: AtomicBool::new(false),
            phases: Mutex::new(Phases::default()),
            threads: n,
            fs_dedupe: fs == "NTFS" || fs == "ReFS",
            cluster,
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

    /// La racine du scan est illisible (aucun contenu, pas une simple erreur partielle).
    pub fn root_denied(&self) -> bool {
        self.shared.root_denied.load(Relaxed)
    }

    /// Nombre de workers lancés.
    pub fn threads(&self) -> usize {
        self.shared.threads
    }

    /// Phases cumulées des workers terminés (toutes après `Msg::Done`).
    pub fn phases(&self) -> Phases {
        *self.shared.phases.lock().unwrap()
    }
}

impl Drop for Scan {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn worker(sh: &Shared, tasks: &Receiver<Task>) {
    let mut ph = Phases::default();
    loop {
        if sh.cancel.load(Relaxed) {
            break;
        }
        if sh.paused.load(Relaxed) {
            std::thread::sleep(Duration::from_millis(30));
            continue;
        }
        match tasks.recv_timeout(Duration::from_millis(5)) {
            Ok(task) => {
                // P4 : le worker enchaîne sur le sous-dossier qu'il a gardé (parcours en profondeur).
                let mut next = Some(task);
                while let Some((id, path)) = next.take() {
                    next = scan_dir(sh, id, path, &mut ph);
                    sh.pending.fetch_sub(1, Relaxed);
                    if next.is_some() && (sh.cancel.load(Relaxed) || sh.paused.load(Relaxed)) {
                        sh.tasks.send(next.take().unwrap()).ok(); // toujours compté dans `pending`
                    }
                }
            }
            Err(_) if sh.pending.load(Relaxed) == 0 => break,
            Err(_) => {}
        }
    }
    *sh.phases.lock().unwrap() += ph;
    if sh.alive.fetch_sub(1, Relaxed) == 1 {
        sh.results.send(Msg::Done).ok();
    }
}

/// Système de fichiers et taille de cluster du volume de `p` ; ("", 0) si inconnus.
pub(crate) fn volume_info(p: &std::path::Path) -> (String, u64) {
    #[cfg(windows)]
    return win::volume_info(p);
    #[cfg(not(windows))]
    {
        let _ = p;
        (String::new(), 0)
    }
}

/// Arrondi au multiple de `cluster` supérieur (identité si `cluster == 0`).
pub(crate) fn round_up(n: u64, cluster: u64) -> u64 {
    if cluster == 0 {
        n
    } else {
        n.div_ceil(cluster) * cluster
    }
}

/// Taille sur disque d'un fichier depuis les données d'énumération ; `None` = allocation
/// masquée (WOF), il faut interroger le fichier.
fn on_disk(eof: u64, alloc: u64, attr: u32, cluster: u64) -> Option<u64> {
    // SPARSE | REPARSE_POINT | OFFLINE | RECALL_ON_OPEN | RECALL_ON_DATA_ACCESS : 0 est crédible.
    const KNOWN_ZERO: u32 = 0x200 | 0x400 | 0x1000 | 0x4_0000 | 0x40_0000;
    if eof > 0 && alloc == 0 && attr & KNOWN_ZERO == 0 {
        None
    } else if cluster > 0 && alloc < cluster {
        Some(0) // résident dans la MFT (ou intégré sur ReFS), comme l'Explorateur
    } else {
        Some(alloc)
    }
}

/// Entrée à ne pas suivre ni compter : dossiers à point d'analyse (jonctions, points de
/// montage) et liens « name surrogate » (liens symboliques). `tag` n'a de sens que si `attr & 0x400`.
fn skipped(attr: u32, tag: u32) -> bool {
    attr & 0x400 != 0 && (attr & 0x10 != 0 || tag & 0x2000_0000 != 0)
}

/// FILETIME UTC (100 ns depuis 1601) -> secondes Unix bornées à `u32`.
fn filetime_to_unix(ft: i64) -> u32 {
    (ft / 10_000_000 - 11_644_473_600).clamp(0, u32::MAX as i64) as u32
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

/// Parcours `std::fs` : repli si l'énumération par handle est refusée, et hors Windows.
/// Taille sur disque = logique arrondie au cluster, pas de dédoublonnage.
fn scan_dir_std(path: &std::path::Path, cluster: u64, errors: &mut u64) -> Option<Vec<Entry>> {
    let rd = std::fs::read_dir(path).ok()?;
    let mut entries = Vec::new();
    for e in rd.flatten() {
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        let Ok(m) = e.metadata() else {
            *errors += 1;
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
        let is_dir = m.is_dir();
        let size = if is_dir { 0 } else { m.len() };
        entries.push(Entry {
            name: e.file_name().to_string_lossy().into_owned(),
            is_dir,
            size,
            alloc: round_up(size, cluster),
            file_id: 0,
            mtime,
        });
    }
    Some(entries)
}

/// Lit un dossier, envoie son lot puis met ses sous-dossiers en file, sauf le dernier, renvoyé
/// pour être traité par le même worker (déjà compté dans `pending`).
fn scan_dir(sh: &Shared, id: u32, path: PathBuf, ph: &mut Phases) -> Option<Task> {
    let mut errors = 0;
    ph.dirs_read += 1;
    #[cfg(windows)]
    let entries = match win::read_dir(&path, sh.cluster, sh.fs_dedupe, &mut errors, ph) {
        Ok(v) => Some(v),
        Err(win::Fail::Open) => None,
        Err(win::Fail::Unsupported) => scan_dir_std(&path, sh.cluster, &mut errors),
    };
    #[cfg(not(windows))]
    let entries = scan_dir_std(&path, sh.cluster, &mut errors);
    errors += entries.is_none() as u64; // dossier illisible
    if id == 0 && entries.is_none() {
        sh.root_denied.store(true, Relaxed);
    }
    if errors > 0 {
        sh.errors.fetch_add(errors, Relaxed);
    }
    let entries = entries?;
    if entries.is_empty() {
        return None;
    }
    let t = Instant::now();
    let first = sh.next_id.fetch_add(entries.len() as u32, Relaxed);
    let mut subdirs: Vec<Task> = entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.is_dir)
        .map(|(i, e)| (first + i as u32, path.join(&e.name)))
        .collect();
    sh.pending.fetch_add(subdirs.len(), Relaxed);
    sh.results.send(Msg::Batch(Batch { parent: id, first, entries })).ok(); // le parent avant ses enfants
    let keep = subdirs.pop();
    for d in subdirs {
        sh.tasks.send(d).ok();
    }
    ph.send += ns(t);
    keep
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn on_disk_rules() {
        const MIB: u64 = 1 << 20;
        let c = 4096;
        assert_eq!(on_disk(10_000, 12_288, 0x20, c), Some(12_288));
        assert_eq!(on_disk(100, 104, 0x20, c), Some(0));
        assert_eq!(on_disk(0, 0, 0x20, c), Some(0));
        assert_eq!(on_disk(10 * MIB, 65_536, 0x220, c), Some(65_536));
        assert_eq!(on_disk(10 * MIB, 0, 0x220, c), Some(0));
        assert_eq!(on_disk(MIB, 65_536, 0x820, c), Some(65_536));
        assert_eq!(on_disk(MIB, 0, 0x20, c), None);
        assert_eq!(on_disk(MIB, 0, 0x40_0420, c), Some(0));
        assert_eq!(on_disk(100, 104, 0x20, 0), Some(104));
    }

    #[test]
    fn worker_count_env() {
        assert_eq!(worker_count(None, 20), 8);
        assert_eq!(worker_count(None, 6), 6);
        assert_eq!(worker_count(None, 1), 4);
        assert_eq!(worker_count(Some("8"), 20), 8);
        assert_eq!(worker_count(Some("0"), 20), 8);
        assert_eq!(worker_count(Some("65"), 20), 8);
        assert_eq!(worker_count(Some("x"), 20), 8);
        assert_eq!(worker_count(Some("64"), 2), 64);
    }

    #[test]
    fn round_up_cluster() {
        assert_eq!(round_up(10_000, 4096), 12_288);
        assert_eq!(round_up(8192, 4096), 8192);
        assert_eq!(round_up(0, 4096), 0);
        assert_eq!(round_up(69_000, 0), 69_000);
    }

    #[test]
    fn skipped_rules() {
        assert!(skipped(0x410, 0xA000_0003));
        assert!(skipped(0x420, 0xA000_000C));
        assert!(!skipped(0x420, 0x9000_001A));
        assert!(!skipped(0x20, 0));
        assert!(!skipped(0x10, 0));
    }

    #[test]
    fn filetime_to_unix() {
        assert_eq!(super::filetime_to_unix(0), 0);
        assert_eq!(super::filetime_to_unix(116_444_736_000_000_000), 0);
        assert_eq!(super::filetime_to_unix(116_444_736_000_000_000 + 10_000_000 * 1_704_067_200), 1_704_067_200);
    }
}

/// Tests sur fichiers réels : chaque test crée son propre dossier neuf sous `temp_dir()` et
/// ne supprime que lui (garde `Drop`, y compris en cas d'échec).
#[cfg(all(test, windows))]
mod fs_tests {
    use super::tree::{Tree, NONE};
    use super::*;
    use std::path::Path;
    use std::process::Command;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> TempDir {
            TempDir::with("f04", name)
        }

        fn f05(name: &str) -> TempDir {
            TempDir::with("f05", name)
        }

        fn with(card: &str, name: &str) -> TempDir {
            let nanos = std::time::SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
            let p = std::env::temp_dir().join(format!("optisize-{card}-{name}-{}-{nanos}", std::process::id()));
            std::fs::create_dir(&p).unwrap(); // échoue si le dossier existe déjà
            TempDir(p)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            // Ne traverse pas les jonctions (std >= 1.58) ; ne touche que ce dossier.
            std::fs::remove_dir_all(&self.0).ok();
        }
    }

    /// Comme `--bench` : scan complet ingéré dans un `Tree`.
    fn scan_tree(dir: &Path) -> Tree {
        scan_with(Scan::start(dir.into()), dir).0
    }

    fn scan_with(scan: Scan, dir: &Path) -> (Tree, Scan) {
        let mut tree = Tree::new(dir.into());
        while let Ok(m) = scan.rx.recv() {
            match m {
                Msg::Batch(b) => tree.ingest(b.parent, b.first, &b.entries),
                Msg::Done => break,
            }
        }
        (tree, scan)
    }

    fn find(t: &Tree, name: &str) -> Option<usize> {
        (1..t.nodes.len()).find(|&id| t.nodes[id].parent != NONE && t.name(id as u32) == name)
    }

    fn vol() -> (String, u64) {
        volume_info(&std::env::temp_dir())
    }

    fn compressible(p: &Path) {
        std::fs::write(p, b"abcdefgh".repeat(1 << 17)).unwrap(); // 1 Mio
    }

    fn compact(args: &[&str], f: &Path) -> bool {
        Command::new("compact").args(args).arg(f).output().is_ok_and(|o| o.status.success())
    }

    #[test]
    fn volume_info_of_temp() {
        let (fs, c) = vol();
        assert!(!fs.is_empty());
        assert!(c >= 512 && c.is_power_of_two(), "{c}");
    }

    #[test]
    fn scan_plain_file_rounds_to_cluster() {
        let d = TempDir::new("plain");
        std::fs::write(d.0.join("f.bin"), vec![1u8; 10_000]).unwrap();
        let t = scan_tree(&d.0);
        let id = find(&t, "f.bin").unwrap();
        assert_eq!((t.nodes[id].size, t.nodes[id].alloc), (10_000, round_up(10_000, vol().1)));
    }

    #[test]
    fn scan_tiny_file_zero_on_disk() {
        let (fs, _) = vol();
        if fs != "NTFS" && fs != "ReFS" {
            eprintln!("ignoré : {fs}");
            return;
        }
        let d = TempDir::new("tiny");
        std::fs::write(d.0.join("t.txt"), [7u8; 100]).unwrap();
        let t = scan_tree(&d.0);
        let id = find(&t, "t.txt").unwrap();
        assert_eq!((t.nodes[id].size, t.nodes[id].alloc), (100, 0));
    }

    #[test]
    fn scan_sparse_file() {
        use std::io::Write;
        use std::os::windows::io::AsRawHandle;
        #[link(name = "kernel32")]
        extern "system" {
            fn DeviceIoControl(
                h: *mut std::ffi::c_void,
                code: u32,
                inb: *mut std::ffi::c_void,
                inl: u32,
                outb: *mut std::ffi::c_void,
                outl: u32,
                ret: *mut u32,
                ov: *mut std::ffi::c_void,
            ) -> i32;
        }
        let null = std::ptr::null_mut;
        let d = TempDir::new("sparse");
        let p = d.0.join("s.bin");
        {
            let mut f = std::fs::File::create(&p).unwrap();
            let mut ret = 0u32;
            // FSCTL_SET_SPARSE (sans élévation)
            let ok = unsafe { DeviceIoControl(f.as_raw_handle(), 0x0009_00C4, null(), 0, null(), 0, &mut ret, null()) };
            assert_ne!(ok, 0, "FSCTL_SET_SPARSE");
            f.write_all(&[1u8; 65_536]).unwrap();
            f.set_len(10 << 20).unwrap();
            f.sync_all().unwrap();
        }
        let t = scan_tree(&d.0);
        let id = find(&t, "s.bin").unwrap();
        assert_eq!(t.nodes[id].size, 10 << 20);
        assert!(t.nodes[id].alloc < 1 << 20, "{}", t.nodes[id].alloc);
    }

    #[test]
    fn scan_compressed_file() {
        if vol().0 != "NTFS" {
            eprintln!("ignoré : pas NTFS");
            return;
        }
        let d = TempDir::new("lznt1");
        let p = d.0.join("c.txt");
        compressible(&p);
        assert!(compact(&["/c"], &p), "compact /c");
        let t = scan_tree(&d.0);
        let id = find(&t, "c.txt").unwrap();
        assert_eq!(t.nodes[id].size, 1 << 20);
        assert!(t.nodes[id].alloc < 256 << 10, "{}", t.nodes[id].alloc);
    }

    #[test]
    fn scan_wof_file() {
        if vol().0 != "NTFS" {
            eprintln!("ignoré : pas NTFS");
            return;
        }
        let d = TempDir::new("wof");
        let p = d.0.join("w.txt");
        compressible(&p);
        if !compact(&["/c", "/exe:xpress4k"], &p) {
            eprintln!("ignoré : compact /exe refusé");
            return;
        }
        let t = scan_tree(&d.0);
        let a = t.nodes[find(&t, "w.txt").unwrap()].alloc;
        assert!(a > 0 && a < 256 << 10, "{a}");
    }

    #[test]
    fn scan_hard_link_counted_once() {
        let d = TempDir::new("hardlink");
        std::fs::create_dir(d.0.join("a")).unwrap();
        std::fs::create_dir(d.0.join("b")).unwrap();
        std::fs::write(d.0.join("a").join("f.bin"), vec![1u8; 10_000]).unwrap();
        std::fs::hard_link(d.0.join("a").join("f.bin"), d.0.join("b").join("g.bin")).unwrap();
        let t = scan_tree(&d.0);
        let r = &t.nodes[0];
        assert_eq!((r.files, r.size, r.alloc), (2, 20_000, round_up(10_000, vol().1)));
    }

    #[test]
    fn scan_does_not_follow_junction() {
        let d = TempDir::new("junction");
        let (target, link) = (d.0.join("cible"), d.0.join("lien"));
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("x.bin"), [1u8; 5000]).unwrap();
        let out = Command::new("cmd").arg("/c").arg("mklink").arg("/J").arg(&link).arg(&target).output().unwrap();
        assert!(out.status.success(), "mklink /J");
        let t = scan_tree(&d.0);
        std::fs::remove_dir(&link).unwrap(); // retire la jonction seule, avant le nettoyage
        assert_eq!((t.nodes[0].files, t.nodes[0].size), (1, 5000));
        assert!(find(&t, "lien").is_none());
    }

    #[test]
    fn scan_long_path() {
        let d = TempDir::new("long");
        let mut p = d.0.clone();
        while p.as_os_str().len() <= 300 {
            p.push("dossier_assez_long_pour_depasser_max_path");
        }
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(p.join("é.txt"), [1u8; 3000]).unwrap();
        let t = scan_tree(&d.0);
        let id = find(&t, "é.txt").expect("fichier au fond trouvé");
        assert_eq!(t.nodes[id].size, 3000);
        assert!(t.nodes[id].alloc == 0 || t.nodes[id].alloc == round_up(3000, vol().1));
        assert_eq!((t.nodes[0].files, t.nodes[0].size), (1, 3000));
    }

    #[test]
    fn scan_matches_std_counts() {
        let d = TempDir::new("counts");
        for i in 0..50usize {
            let sub = d.0.join(format!("d{i}"));
            std::fs::create_dir(&sub).unwrap();
            for j in 0..4usize {
                std::fs::write(sub.join(format!("f{j}.dat")), vec![0u8; i * 997 + j * 131]).unwrap();
            }
        }
        fn walk(p: &Path, acc: &mut (u32, u32, u64)) {
            for e in std::fs::read_dir(p).unwrap().flatten() {
                let m = e.metadata().unwrap();
                if m.is_dir() {
                    acc.1 += 1;
                    walk(&e.path(), acc);
                } else {
                    acc.0 += 1;
                    acc.2 += m.len();
                }
            }
        }
        let mut want = (0, 0, 0);
        walk(&d.0, &mut want);
        let t = scan_tree(&d.0);
        assert_eq!((t.nodes[0].files, t.nodes[0].dirs, t.nodes[0].size), want);
    }

    /// Comptes (fichiers, dossiers, taille logique) d'un parcours `std::fs`.
    fn std_counts(p: &Path) -> (u32, u32, u64) {
        fn walk(p: &Path, acc: &mut (u32, u32, u64)) {
            for e in std::fs::read_dir(p).unwrap().flatten() {
                let m = e.metadata().unwrap();
                if m.is_dir() {
                    acc.1 += 1;
                    walk(&e.path(), acc);
                } else {
                    acc.0 += 1;
                    acc.2 += m.len();
                }
            }
        }
        let mut acc = (0, 0, 0);
        walk(p, &mut acc);
        acc
    }

    /// Refus de lister à « Tout le monde » posé sur un dossier créé par le test ; `Drop` retire
    /// le refus puis le dossier, dans cet ordre.
    struct Denied(TempDir);

    impl Drop for Denied {
        fn drop(&mut self) {
            Command::new("icacls").arg(&self.0 .0).args(["/remove:d", "*S-1-1-0"]).output().ok();
        }
    }

    #[test]
    fn scan_reports_root_denied() {
        let d = TempDir::f05("denied");
        std::fs::write(d.0.join("f.txt"), [1u8; 10]).unwrap();
        let ok = Command::new("icacls").arg(&d.0).args(["/deny", "*S-1-1-0:(RD)"]).output().is_ok_and(|o| o.status.success());
        let d = Denied(d);
        if !ok {
            eprintln!("ignoré : icacls /deny refusé");
            return;
        }
        if std::fs::read_dir(&d.0 .0).is_ok() {
            eprintln!("ignoré : le refus n'empêche pas ce processus de lister (privilège de sauvegarde actif ?)");
            return;
        }
        let (t, s) = scan_with(Scan::start(d.0 .0.clone()), &d.0 .0);
        assert!(s.root_denied());
        assert!(s.errors() >= 1);
        assert_eq!((t.nodes[0].files, t.nodes[0].dirs, t.nodes[0].child_count), (0, 0, 0));
    }

    #[test]
    fn scan_root_ok_not_denied() {
        let d = TempDir::f05("readable");
        std::fs::write(d.0.join("f.txt"), [1u8; 10]).unwrap();
        let (t, s) = scan_with(Scan::start(d.0.clone()), &d.0);
        assert!(!s.root_denied());
        assert_eq!(s.errors(), 0);
        assert_eq!(t.nodes[0].files, 1);
    }

    #[test]
    fn phases_are_counted() {
        let d = TempDir::f05("phases");
        for i in 0..20 {
            let sub = d.0.join(format!("d{i}"));
            std::fs::create_dir(&sub).unwrap();
            for j in 0..5 {
                std::fs::write(sub.join(format!("f{j}")), [0u8; 100]).unwrap();
            }
        }
        let (_, s) = scan_with(Scan::start(d.0.clone()), &d.0);
        let p = s.phases();
        assert_eq!(p.dirs_read, 21);
        assert!(p.open + p.query > 0);
        assert_eq!(p.wof_calls, 0);
    }

    #[test]
    fn scan_terminates_many_threads_few_dirs() {
        let d = TempDir::f05("threads");
        for a in 0..3 {
            for b in 0..3 {
                let sub = d.0.join(format!("a{a}")).join(format!("b{b}")).join("c");
                std::fs::create_dir_all(&sub).unwrap();
                std::fs::write(sub.join("f.dat"), vec![0u8; a * 1000 + b * 10]).unwrap();
                std::fs::write(d.0.join(format!("a{a}")).join(format!("g{b}")), [0u8; 7]).unwrap();
            }
        }
        let want = std_counts(&d.0);
        for n in [1, 2, 48] {
            let t0 = Instant::now();
            let (t, s) = scan_with(Scan::start_with(d.0.clone(), n), &d.0);
            assert!(t0.elapsed() < Duration::from_secs(5), "{n} workers : trop long");
            assert_eq!(s.threads(), n);
            assert_eq!((t.nodes[0].files, t.nodes[0].dirs, t.nodes[0].size), want, "{n} workers");
        }
    }
}
