//! Arbre compact : arène `Vec<Node>` indexée par `u32`, noms stockés dans un seul `String`.
//! Les enfants d'un dossier sont contigus (`first_child..first_child+child_count`).
//! Tailles / compteurs sont agrégés en continu à chaque lot reçu.

use std::collections::HashSet;
use std::path::PathBuf;

pub const NONE: u32 = u32::MAX;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Kind {
    Video,
    Audio,
    Image,
    Archive,
    Document,
    Code,
    Exec,
    Other,
}

pub const KINDS: [Kind; 8] = [
    Kind::Video,
    Kind::Audio,
    Kind::Image,
    Kind::Archive,
    Kind::Document,
    Kind::Code,
    Kind::Exec,
    Kind::Other,
];

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Video => "Vidéo",
            Kind::Audio => "Audio",
            Kind::Image => "Images",
            Kind::Archive => "Archives",
            Kind::Document => "Documents",
            Kind::Code => "Code",
            Kind::Exec => "Exécutables",
            Kind::Other => "Autres",
        }
    }

    pub fn from_name(name: &str) -> Kind {
        let Some((_, ext)) = name.rsplit_once('.') else {
            return Kind::Other;
        };
        match ext.to_ascii_lowercase().as_str() {
            "mp4" | "mkv" | "avi" | "mov" | "wmv" | "webm" | "m4v" | "flv" | "mpg" | "mpeg" => Kind::Video,
            "mp3" | "flac" | "wav" | "ogg" | "m4a" | "aac" | "wma" | "opus" => Kind::Audio,
            "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "tif" | "tiff" | "svg" | "psd" | "heic" | "raw" | "ico" => {
                Kind::Image
            }
            "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "iso" | "cab" | "zst" => Kind::Archive,
            "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "md" | "odt" | "csv" | "rtf" => {
                Kind::Document
            }
            "rs" | "c" | "h" | "cpp" | "hpp" | "cs" | "js" | "ts" | "py" | "java" | "go" | "html" | "css" | "json"
            | "toml" | "yaml" | "yml" | "xml" | "rlib" | "rmeta" | "pdb" | "o" | "obj" => Kind::Code,
            "exe" | "dll" | "msi" | "sys" | "bat" | "ps1" | "lib" => Kind::Exec,
            _ => Kind::Other,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Node {
    /// Taille logique (fin de fichier ; chaque nom de lien physique compté).
    pub size: u64,
    /// Taille sur disque (allouée ; un lien physique compté une fois par scan).
    pub alloc: u64,
    pub name_off: u32,
    pub parent: u32,
    pub first_child: u32,
    pub child_count: u32,
    pub files: u32,
    pub dirs: u32,
    pub mtime: u32,
    pub name_len: u16,
    pub kind: Kind,
    pub is_dir: bool,
}

impl Node {
    /// Emplacement réservé (id alloué par un worker, lot pas encore reçu).
    const HOLE: Node = Node {
        size: 0,
        alloc: 0,
        name_off: 0,
        parent: NONE,
        first_child: 0,
        child_count: 0,
        files: 0,
        dirs: 0,
        mtime: 0,
        name_len: 0,
        kind: Kind::Other,
        is_dir: false,
    };
}

/// Entrée brute produite par un worker de scan.
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    /// Taille sur disque déjà calculée par le worker.
    pub alloc: u64,
    /// Identifiant de fichier à dédoublonner (liens physiques), 0 = jamais dédoublonné.
    pub file_id: u64,
    pub mtime: u32,
}

pub struct Tree {
    pub nodes: Vec<Node>,
    names: String,
    pub root_path: PathBuf,
    type_sizes: [u64; 8],
    type_alloc: [u64; 8],
    /// Mode d'affichage : taille sur disque (`true`) ou logique.
    on_disk: bool,
    /// Identifiants de fichier déjà comptés dans ce scan ; vidé par `end_scan`.
    links: HashSet<u64>,
    /// Incrémenté à chaque lot et à chaque bascule de mode : sert d'invalidation de cache à l'UI.
    pub version: u64,
}

impl Tree {
    pub fn new(root_path: PathBuf) -> Tree {
        let name = root_path.to_string_lossy().into_owned();
        let mut t = Tree {
            nodes: Vec::new(),
            names: String::new(),
            root_path,
            type_sizes: [0; 8],
            type_alloc: [0; 8],
            on_disk: true,
            links: HashSet::new(),
            version: 0,
        };
        t.names.push_str(&name);
        let mut root = Node::HOLE;
        root.is_dir = true;
        root.name_len = name.len().min(u16::MAX as usize) as u16;
        t.nodes.push(root);
        t
    }

    /// Insère les enfants de `parent` aux ids `first..first+entries.len()` et propage les totaux.
    ///
    /// Liens physiques : le premier nom reçu d'un `file_id` porte l'allocation, les suivants ont
    /// `alloc = 0` (la taille logique est gardée). Les autres noms peuvent avoir une taille périmée
    /// dans l'entrée de répertoire (NTFS ne met à jour que le nom utilisé pour écrire). Les fichiers
    /// clonés par blocs (ReFS) rapportent chacun leur allocation et sont comptés en entier.
    // shortcut: le dossier qui porte l'allocation dépend de l'ordre des workers (total racine
    // déterministe) ; passer à une attribution au prorata du nombre de liens si on s'en plaint.
    pub fn ingest(&mut self, parent: u32, first: u32, entries: &[Entry]) {
        let first_i = first as usize;
        let end = first_i + entries.len();
        if self.nodes.len() < end {
            self.nodes.resize(end, Node::HOLE);
        }
        let (mut fsize, mut falloc, mut fcount, mut dcount) = (0u64, 0u64, 0u32, 0u32);
        for (i, e) in entries.iter().enumerate() {
            let name_off = self.names.len() as u32;
            let name = &e.name[..e.name.len().min(u16::MAX as usize)];
            self.names.push_str(name);
            let kind = if e.is_dir { Kind::Other } else { Kind::from_name(&e.name) };
            let alloc = if e.is_dir || (e.file_id != 0 && !self.links.insert(e.file_id)) { 0 } else { e.alloc };
            self.nodes[first_i + i] = Node {
                size: if e.is_dir { 0 } else { e.size },
                alloc,
                name_off,
                parent,
                first_child: 0,
                child_count: 0,
                files: 0,
                dirs: 0,
                mtime: e.mtime,
                name_len: name.len() as u16,
                kind,
                is_dir: e.is_dir,
            };
            if e.is_dir {
                dcount += 1;
            } else {
                fsize += e.size;
                falloc += alloc;
                fcount += 1;
                self.type_sizes[kind as usize] += e.size;
                self.type_alloc[kind as usize] += alloc;
            }
        }
        let p = &mut self.nodes[parent as usize];
        p.first_child = first;
        p.child_count = entries.len() as u32;
        let mut cur = parent;
        while cur != NONE {
            let n = &mut self.nodes[cur as usize];
            n.size += fsize;
            n.alloc += falloc;
            n.files += fcount;
            n.dirs += dcount;
            cur = n.parent;
        }
        self.version += 1;
    }

    /// Taille de `id` dans le mode affiché.
    pub fn size(&self, id: u32) -> u64 {
        let n = &self.nodes[id as usize];
        if self.on_disk {
            n.alloc
        } else {
            n.size
        }
    }

    /// Total d'un type de fichier dans le mode affiché.
    pub fn type_size(&self, k: Kind) -> u64 {
        if self.on_disk {
            self.type_alloc[k as usize]
        } else {
            self.type_sizes[k as usize]
        }
    }

    pub fn on_disk(&self) -> bool {
        self.on_disk
    }

    pub fn set_on_disk(&mut self, b: bool) {
        if self.on_disk != b {
            self.on_disk = b;
            self.version += 1;
        }
    }

    /// Fin (ou arrêt) du scan : libère l'ensemble des identifiants de liens physiques.
    pub fn end_scan(&mut self) {
        self.links = HashSet::new();
    }

    pub fn name(&self, id: u32) -> &str {
        let n = &self.nodes[id as usize];
        let off = if id == 0 { 0 } else { n.name_off as usize };
        &self.names[off..off + n.name_len as usize]
    }

    pub fn children(&self, id: u32) -> std::ops::Range<u32> {
        let n = &self.nodes[id as usize];
        n.first_child..n.first_child + n.child_count
    }

    pub fn path(&self, id: u32) -> PathBuf {
        let mut parts = Vec::new();
        let mut cur = id;
        while cur != 0 && cur != NONE {
            parts.push(self.name(cur));
            cur = self.nodes[cur as usize].parent;
        }
        let mut p = self.root_path.clone();
        for part in parts.iter().rev() {
            p.push(part);
        }
        p
    }

    /// Nombre d'ancêtres de `id` (0 pour la racine).
    pub fn depth(&self, id: u32) -> usize {
        let (mut d, mut cur) = (0, id);
        while cur != 0 && cur != NONE {
            d += 1;
            cur = self.nodes[cur as usize].parent;
        }
        d
    }

    pub fn mem_bytes(&self) -> usize {
        // ≈ 9 octets par case du HashSet (clé + octet de contrôle)
        self.nodes.capacity() * std::mem::size_of::<Node>() + self.names.capacity() + self.links.capacity() * 9
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(name: &str, is_dir: bool, size: u64) -> Entry {
        Entry { name: name.into(), is_dir, size, alloc: size, file_id: 0, mtime: 0 }
    }

    fn f(name: &str, size: u64, alloc: u64, file_id: u64) -> Entry {
        Entry { name: name.into(), is_dir: false, size, alloc, file_id, mtime: 0 }
    }

    #[test]
    fn node_is_48_bytes() {
        assert_eq!(std::mem::size_of::<Node>(), 48);
    }

    #[test]
    fn aggregates_alloc_up_to_root() {
        let mut t = Tree::new(PathBuf::from("R"));
        t.ingest(0, 1, &[e("a", true, 0), f("f1.mp4", 100, 4096, 0)]);
        t.ingest(1, 3, &[e("b", true, 0), f("f2", 50, 0, 0), f("f3.zip", 5000, 8192, 0)]);
        t.ingest(3, 6, &[f("f4", 10_000, 12_288, 0)]);
        let sa = |id: usize| (t.nodes[id].size, t.nodes[id].alloc);
        assert_eq!(sa(3), (10_000, 12_288));
        assert_eq!(sa(1), (15_050, 20_480));
        assert_eq!(sa(0), (15_150, 24_576));
        assert_eq!(t.type_size(Kind::Archive), 8192);
        t.set_on_disk(false);
        assert_eq!(t.type_size(Kind::Archive), 5000);
    }

    #[test]
    fn hard_link_alloc_counted_once() {
        let mut t = Tree::new(PathBuf::from("R"));
        t.ingest(0, 1, &[e("a", true, 0), e("b", true, 0)]);
        t.ingest(1, 3, &[f("f.bin", 10_000, 12_288, 42)]);
        t.ingest(2, 4, &[f("g.bin", 10_000, 12_288, 42)]);
        assert_eq!((t.nodes[0].size, t.nodes[0].alloc), (20_000, 12_288));
        assert_eq!((t.nodes[4].size, t.nodes[4].alloc), (10_000, 0));
    }

    #[test]
    fn file_id_zero_never_deduped() {
        let mut t = Tree::new(PathBuf::from("R"));
        t.ingest(0, 1, &[f("x", 10_000, 12_288, 0), f("y", 10_000, 12_288, 0)]);
        assert_eq!(t.nodes[0].alloc, 24_576);
    }

    #[test]
    fn toggle_switches_sizes_and_bumps_version() {
        let mut t = Tree::new(PathBuf::from("R"));
        t.ingest(0, 1, &[f("x.mp4", 100, 4096, 0)]);
        assert!(t.on_disk());
        assert_eq!((t.size(0), t.size(1), t.type_size(Kind::Video)), (4096, 4096, 4096));
        let v = t.version;
        t.set_on_disk(false);
        assert_eq!((t.size(0), t.size(1), t.type_size(Kind::Video)), (100, 100, 100));
        assert_eq!(t.version, v + 1);
        t.set_on_disk(false);
        assert_eq!(t.version, v + 1);
    }

    #[test]
    fn end_scan_frees_link_set() {
        let mut t = Tree::new(PathBuf::from("R"));
        let files: Vec<Entry> = (1..=1000).map(|i| f(&format!("f{i}"), 10, 4096, i)).collect();
        t.ingest(0, 1, &files);
        let (before, total) = (t.mem_bytes(), (t.nodes[0].size, t.nodes[0].alloc));
        t.end_scan();
        assert!(t.mem_bytes() < before);
        assert_eq!((t.nodes[0].size, t.nodes[0].alloc), total);
    }

    #[test]
    fn aggregates_sizes_and_counts_up_to_root() {
        let mut t = Tree::new(PathBuf::from("R"));
        // racine : a/ (dossier), f1 (100)
        t.ingest(0, 1, &[e("a", true, 0), e("f1.mp4", false, 100)]);
        assert_eq!(t.nodes[0].size, 100);
        // a/ : b/ (dossier), f2 (50), f3 (25)
        t.ingest(1, 3, &[e("b", true, 0), e("f2", false, 50), e("f3.zip", false, 25)]);
        // a/b : f4 (10)
        t.ingest(3, 6, &[e("f4", false, 10)]);

        assert_eq!(t.nodes[3].size, 10);
        assert_eq!(t.nodes[1].size, 85);
        assert_eq!(t.nodes[0].size, 185);
        assert_eq!((t.nodes[0].files, t.nodes[0].dirs), (4, 2));
        assert_eq!((t.nodes[1].files, t.nodes[1].dirs), (3, 1));
        t.set_on_disk(false);
        assert_eq!(t.type_size(Kind::Video), 100);
        assert_eq!(t.type_size(Kind::Archive), 25);
        assert_eq!(t.name(6), "f4");
        assert_eq!(t.path(6), PathBuf::from("R").join("a").join("b").join("f4"));
        assert_eq!(t.depth(6), 3);
    }

    #[test]
    fn out_of_order_ids_leave_holes_then_fill() {
        let mut t = Tree::new(PathBuf::from("R"));
        t.ingest(0, 1, &[e("a", true, 0)]);
        // lot du dossier a reçu avec des ids plus loin (autre worker a pris 2..5)
        t.ingest(1, 5, &[e("x", false, 7)]);
        assert_eq!(t.nodes.len(), 6);
        assert_eq!(t.nodes[0].size, 7);
        assert_eq!(t.name(5), "x");
    }
}
