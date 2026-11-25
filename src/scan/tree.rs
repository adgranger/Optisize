//! Arbre compact : arène `Vec<Node>` indexée par `u32`, noms stockés dans un seul `String`.
//! Les enfants d'un dossier sont contigus (`first_child..first_child+child_count`).
//! Tailles / compteurs sont agrégés en continu à chaque lot reçu.

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
    pub size: u64,
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
    pub mtime: u32,
}

pub struct Tree {
    pub nodes: Vec<Node>,
    names: String,
    pub root_path: PathBuf,
    pub type_sizes: [u64; 8],
    /// Incrémenté à chaque lot : sert d'invalidation de cache à l'UI.
    pub version: u64,
}

impl Tree {
    pub fn new(root_path: PathBuf) -> Tree {
        let name = root_path.to_string_lossy().into_owned();
        let mut t = Tree { nodes: Vec::new(), names: String::new(), root_path, type_sizes: [0; 8], version: 0 };
        t.names.push_str(&name);
        let mut root = Node::HOLE;
        root.is_dir = true;
        root.name_len = name.len().min(u16::MAX as usize) as u16;
        t.nodes.push(root);
        t
    }

    /// Insère les enfants de `parent` aux ids `first..first+entries.len()` et propage les totaux.
    pub fn ingest(&mut self, parent: u32, first: u32, entries: &[Entry]) {
        let first_i = first as usize;
        let end = first_i + entries.len();
        if self.nodes.len() < end {
            self.nodes.resize(end, Node::HOLE);
        }
        let (mut fsize, mut fcount, mut dcount) = (0u64, 0u32, 0u32);
        for (i, e) in entries.iter().enumerate() {
            let name_off = self.names.len() as u32;
            let name = &e.name[..e.name.len().min(u16::MAX as usize)];
            self.names.push_str(name);
            let kind = if e.is_dir { Kind::Other } else { Kind::from_name(&e.name) };
            self.nodes[first_i + i] = Node {
                size: if e.is_dir { 0 } else { e.size },
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
                fcount += 1;
                self.type_sizes[kind as usize] += e.size;
            }
        }
        let p = &mut self.nodes[parent as usize];
        p.first_child = first;
        p.child_count = entries.len() as u32;
        let mut cur = parent;
        while cur != NONE {
            let n = &mut self.nodes[cur as usize];
            n.size += fsize;
            n.files += fcount;
            n.dirs += dcount;
            cur = n.parent;
        }
        self.version += 1;
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
        self.nodes.capacity() * std::mem::size_of::<Node>() + self.names.capacity()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(name: &str, is_dir: bool, size: u64) -> Entry {
        Entry { name: name.into(), is_dir, size, mtime: 0 }
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
        assert_eq!(t.type_sizes[Kind::Video as usize], 100);
        assert_eq!(t.type_sizes[Kind::Archive as usize], 25);
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
