//! Énumération Windows par handle de dossier (`FileIdBothDirectoryInfo`) : taille allouée et
//! identifiant de fichier sans aucun appel par fichier (même coût que `read_dir`).

use super::tree::Entry;
use super::{filetime_to_unix, on_disk, round_up, skipped};
use std::cell::RefCell;
use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::null_mut;

/// `FILE_ID_BOTH_DIR_INFO` ; le nom UTF-16 suit l'en-tête (offset 104).
#[repr(C)]
#[derive(Clone, Copy)]
#[allow(dead_code)] // disposition noyau : tous les champs ne sont pas lus
pub(super) struct FileIdBothDirInfo {
    next: u32, // 0 : dernière entrée du tampon
    index: u32,
    creation: i64,
    access: i64,
    write: i64,
    change: i64,
    eof: i64,
    alloc: i64,
    attr: u32,
    name_len: u32, // en octets
    ea_size: u32,  // = balise de point d'analyse si attr & 0x400
    short_len: i8,
    short_name: [u16; 12],
    file_id: i64,
}

pub(super) const HDR: usize = std::mem::size_of::<FileIdBothDirInfo>();

#[link(name = "kernel32")]
extern "system" {
    fn CreateFileW(name: *const u16, access: u32, share: u32, sa: *mut c_void, disp: u32, flags: u32, tmpl: *mut c_void) -> *mut c_void;
    fn CloseHandle(h: *mut c_void) -> i32;
    fn GetFileInformationByHandleEx(h: *mut c_void, class: u32, buf: *mut c_void, len: u32) -> i32;
    fn GetCompressedFileSizeW(name: *const u16, high: *mut u32) -> u32;
    fn GetLastError() -> u32;
    fn SetLastError(e: u32);
    fn GetVolumePathNameW(name: *const u16, out: *mut u16, len: u32) -> i32;
    fn GetVolumeInformationW(
        root: *const u16,
        vol: *mut u16,
        vol_len: u32,
        serial: *mut u32,
        max_comp: *mut u32,
        flags: *mut u32,
        fs: *mut u16,
        fs_len: u32,
    ) -> i32;
    fn GetDiskFreeSpaceW(root: *const u16, spc: *mut u32, bps: *mut u32, free: *mut u32, total: *mut u32) -> i32;
}

struct Handle(*mut c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

thread_local! {
    /// Tampon d'énumération de 64 Kio aligné sur 8, un par thread worker.
    static BUF: RefCell<Vec<u64>> = RefCell::new(vec![0; 8192]);
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// Chemin absolu -> chemin verbatim terminé par 0 (pas de limite MAX_PATH).
pub(super) fn verbatim(p: &Path) -> Vec<u16> {
    let s: Vec<u16> = p.as_os_str().encode_wide().collect();
    let mut out = if s.starts_with(&wide(r"\\?\")) || s.starts_with(&wide(r"\\.\")) {
        s
    } else if s.starts_with(&wide(r"\\")) {
        let mut o = wide(r"\\?\UNC\");
        o.extend_from_slice(&s[2..]);
        o
    } else {
        let mut o = wide(r"\\?\");
        o.extend(s);
        o
    };
    out.push(0);
    out
}

pub(super) fn volume_info(p: &Path) -> (String, u64) {
    let mut name: Vec<u16> = p.as_os_str().encode_wide().collect();
    name.push(0);
    let mut root = [0u16; 1024];
    let mut fs = [0u16; 64];
    let (mut spc, mut bps, mut free, mut total) = (0u32, 0u32, 0u32, 0u32);
    unsafe {
        if GetVolumePathNameW(name.as_ptr(), root.as_mut_ptr(), root.len() as u32) == 0 {
            return (String::new(), 0);
        }
        let fs_name = if GetVolumeInformationW(
            root.as_ptr(),
            null_mut(),
            0,
            null_mut(),
            null_mut(),
            null_mut(),
            fs.as_mut_ptr(),
            fs.len() as u32,
        ) != 0
        {
            let n = fs.iter().position(|&c| c == 0).unwrap_or(fs.len());
            String::from_utf16_lossy(&fs[..n])
        } else {
            String::new()
        };
        let cluster = if GetDiskFreeSpaceW(root.as_ptr(), &mut spc, &mut bps, &mut free, &mut total) != 0 {
            spc as u64 * bps as u64
        } else {
            0
        };
        (fs_name, cluster)
    }
}

/// Taille compressée (WOF) d'un fichier, `None` si l'appel échoue.
fn compressed_size(p: &Path) -> Option<u64> {
    let w = verbatim(p);
    let mut hi = 0u32;
    unsafe {
        SetLastError(0);
        let lo = GetCompressedFileSizeW(w.as_ptr(), &mut hi);
        if lo == u32::MAX && GetLastError() != 0 {
            return None;
        }
        Some((hi as u64) << 32 | lo as u64)
    }
}

pub(super) enum Fail {
    /// Dossier illisible : à compter en erreur.
    Open,
    /// Classe d'information refusée (pilote tiers, SMB) : repli sur `read_dir`.
    Unsupported,
}

/// Lit un dossier. Les erreurs partielles (après le premier appel) sont ajoutées à `errors`.
pub(super) fn read_dir(path: &Path, cluster: u64, dedupe: bool, errors: &mut u64) -> Result<Vec<Entry>, Fail> {
    let w = verbatim(path);
    // FILE_LIST_DIRECTORY, partage lecture|écriture|suppression, OPEN_EXISTING, FILE_FLAG_BACKUP_SEMANTICS
    let h = unsafe { CreateFileW(w.as_ptr(), 0x0001, 0x7, null_mut(), 3, 0x0200_0000, null_mut()) };
    if h as isize == -1 {
        return Err(Fail::Open);
    }
    let h = Handle(h);
    BUF.with_borrow_mut(|buf| {
        let mut entries = Vec::new();
        let mut first = true;
        loop {
            let ok = unsafe { GetFileInformationByHandleEx(h.0, 10, buf.as_mut_ptr().cast(), (buf.len() * 8) as u32) };
            if ok == 0 {
                match unsafe { GetLastError() } {
                    18 => {} // ERROR_NO_MORE_FILES
                    _ if first => return Err(Fail::Unsupported),
                    _ => *errors += 1,
                }
                break;
            }
            first = false;
            if !parse(buf, path, cluster, dedupe, &mut entries) {
                *errors += 1;
                break;
            }
        }
        Ok(entries)
    })
}

/// Décode un tampon rempli ; `false` si une entrée sort du tampon (on garde ce qui précède).
fn parse(buf: &[u64], dir: &Path, cluster: u64, dedupe: bool, out: &mut Vec<Entry>) -> bool {
    let len = buf.len() * 8;
    let base = buf.as_ptr().cast::<u8>();
    let mut off = 0usize;
    loop {
        if !off.is_multiple_of(8) || off + HDR > len {
            return false;
        }
        let d: FileIdBothDirInfo = unsafe { std::ptr::read_unaligned(base.add(off).cast()) };
        let nl = d.name_len as usize;
        if off + HDR + nl > len {
            return false;
        }
        let name = unsafe { std::slice::from_raw_parts(base.add(off + HDR).cast::<u16>(), nl / 2) };
        if name != [b'.' as u16] && name != [b'.' as u16, b'.' as u16] && !skipped(d.attr, d.ea_size) {
            let name = String::from_utf16_lossy(name);
            let is_dir = d.attr & 0x10 != 0;
            let (size, alloc, file_id) = if is_dir {
                (0, 0, 0)
            } else {
                let eof = d.eof.max(0) as u64;
                let alloc = on_disk(eof, d.alloc.max(0) as u64, d.attr, cluster).unwrap_or_else(|| {
                    round_up(compressed_size(&dir.join(&name)).unwrap_or(eof), cluster)
                });
                let id = d.file_id as u64;
                let keep = dedupe && alloc != 0 && id != 0 && id != u64::MAX;
                (eof, alloc, if keep { id } else { 0 })
            };
            out.push(Entry { name, is_dir, size, alloc, file_id, mtime: filetime_to_unix(d.write) });
        }
        if d.next == 0 {
            return true;
        }
        off += d.next as usize;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dir_info_layout() {
        assert_eq!(std::mem::offset_of!(FileIdBothDirInfo, file_id), 96);
        assert_eq!(HDR, 104);
    }

    #[test]
    fn verbatim_paths() {
        let v = |s: &str| String::from_utf16(&verbatim(Path::new(s))).unwrap();
        assert_eq!(v(r"C:\a"), "\\\\?\\C:\\a\0");
        assert_eq!(v(r"\\srv\sh\x"), "\\\\?\\UNC\\srv\\sh\\x\0");
        assert_eq!(v(r"\\?\C:\a"), "\\\\?\\C:\\a\0");
    }
}
