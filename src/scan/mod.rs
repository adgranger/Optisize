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
    /// Volume NTFS ou ReFS : identifiants de fichier fiables, liens physiques dédoublonnés.
    fs_dedupe: bool,
    /// Taille de cluster du volume (0 si inconnue : ni arrondi ni seuil).
    cluster: u64,
    tasks: Sender<(u32, PathBuf)>,
    results: Sender<Msg>,
}

pub struct Scan {
    pub rx: Receiver<Msg>,
    shared: Arc<Shared>,
}

impl Scan {
    pub fn start(root: PathBuf) -> Scan {
        // Les workers utilisent des chemins verbatim (préfixe `\\?\`) : il les faut absolus.
        let root = std::path::absolute(&root).unwrap_or(root);
        let (fs, cluster) = volume_info(&root);
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

fn scan_dir(sh: &Shared, id: u32, path: PathBuf) {
    let mut errors = 0;
    #[cfg(windows)]
    let entries = match win::read_dir(&path, sh.cluster, sh.fs_dedupe, &mut errors) {
        Ok(v) => Some(v),
        Err(win::Fail::Open) => None,
        Err(win::Fail::Unsupported) => scan_dir_std(&path, sh.cluster, &mut errors),
    };
    #[cfg(not(windows))]
    let entries = scan_dir_std(&path, sh.cluster, &mut errors);
    errors += entries.is_none() as u64; // dossier illisible
    if errors > 0 {
        sh.errors.fetch_add(errors, Relaxed);
    }
    let Some(entries) = entries else { return };
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
            let nanos = std::time::SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
            let p = std::env::temp_dir().join(format!("optisize-f04-{name}-{}-{nanos}", std::process::id()));
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
        let scan = Scan::start(dir.into());
        let mut tree = Tree::new(dir.into());
        while let Ok(m) = scan.rx.recv() {
            match m {
                Msg::Batch(b) => tree.ingest(b.parent, b.first, &b.entries),
                Msg::Done => break,
            }
        }
        tree
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
}
