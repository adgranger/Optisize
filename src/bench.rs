//! `optisize --bench <dossier> [--json] [--profile]` : scan sans interface, mesure et sortie
//! lisible (humaine ou une ligne JSON pour `bench\bench.ps1`).

use crate::{fmt, scan};
use std::time::Instant;

#[derive(Debug, PartialEq)]
pub struct BenchOpts {
    pub path: String,
    pub json: bool,
    pub profile: bool,
}

/// Arguments après `--bench` : le dossier, puis les options dans n'importe quel ordre.
pub fn bench_args(args: &[String]) -> Result<BenchOpts, String> {
    let mut o = BenchOpts { path: args.first().cloned().unwrap_or_else(|| ".".into()), json: false, profile: false };
    for a in args.iter().skip(1) {
        match a.as_str() {
            "--json" => o.json = true,
            "--profile" => o.profile = true,
            x => return Err(format!("option inconnue : {x}")),
        }
    }
    Ok(o)
}

/// Chaîne JSON entre guillemets.
pub fn json_str(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '\\' => o.push_str("\\\\"),
            '"' => o.push_str("\\\""),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

pub struct Report {
    pub files: u64,
    pub dirs: u64,
    pub logical: u64,
    pub on_disk: u64,
    pub secs: f64,
    pub errors: u64,
    pub root_denied: bool,
    pub mem: u64,
    pub threads: usize,
    pub phases: scan::Phases,
    /// Thread consommateur : temps dans `Tree::ingest` et bloqué dans `recv` (µs).
    pub ingest_us: u64,
    pub wait_us: u64,
}

pub fn bench_json(path: &str, r: &Report, profile: bool) -> String {
    let mut s = format!(
        "{{\"version\":\"{}\",\"path\":{},\"files\":{},\"dirs\":{},\"logical\":{},\"on_disk\":{},\"secs\":{:.4},\"errors\":{},\"root_denied\":{},\"mem\":{},\"threads\":{}",
        env!("CARGO_PKG_VERSION"),
        json_str(path),
        r.files,
        r.dirs,
        r.logical,
        r.on_disk,
        r.secs,
        r.errors,
        r.root_denied,
        r.mem,
        r.threads
    );
    if profile {
        let p = &r.phases;
        s += &format!(
            ",\"phases\":{{\"open_us\":{},\"query_us\":{},\"parse_us\":{},\"send_us\":{},\"wof_calls\":{},\"dirs_read\":{},\"ingest_us\":{},\"wait_us\":{}}}",
            p.open / 1000,
            p.query / 1000,
            p.parse / 1000,
            p.send / 1000,
            p.wof_calls,
            p.dirs_read,
            r.ingest_us,
            r.wait_us
        );
    }
    s + "}"
}

/// Exécute la mesure ; renvoie le code de sortie du processus.
pub fn run(args: &[String]) -> i32 {
    let o = match bench_args(args) {
        Ok(o) => o,
        Err(e) => {
            println!("{e}");
            return 2;
        }
    };
    let t0 = Instant::now();
    let s = scan::Scan::start(o.path.as_str().into());
    let mut tree = scan::tree::Tree::new(o.path.as_str().into());
    let (mut ingest, mut wait) = (0u128, 0u128);
    loop {
        let t = Instant::now();
        let Ok(m) = s.rx.recv() else { break };
        wait += t.elapsed().as_nanos();
        match m {
            scan::Msg::Batch(b) => {
                let t = Instant::now();
                tree.ingest(b.parent, b.first, &b.entries);
                ingest += t.elapsed().as_nanos();
            }
            scan::Msg::Done => break,
        }
    }
    let secs = t0.elapsed().as_secs_f64();
    let mem = tree.mem_bytes(); // pic : ensemble des liens physiques compris
    tree.end_scan();
    let root = &tree.nodes[0];
    let r = Report {
        files: root.files as u64,
        dirs: root.dirs as u64,
        logical: root.size,
        on_disk: root.alloc,
        secs,
        errors: s.errors(),
        root_denied: s.root_denied(),
        mem: mem as u64,
        threads: s.threads(),
        phases: s.phases(),
        ingest_us: (ingest / 1000) as u64,
        wait_us: (wait / 1000) as u64,
    };
    if o.json {
        println!("{}", bench_json(&o.path, &r, o.profile));
    } else if r.root_denied {
        println!("Accès refusé à la racine : {} (dossier protégé ; élévation ou application empaquetée requise)", o.path);
    } else {
        let errs = if r.errors > 0 { format!(", {} dossiers ignorés (accès refusé)", fmt::thousands(r.errors)) } else { String::new() };
        println!(
            "{} fichiers, {} dossiers, {} logique, {} sur disque en {secs:.2} s => {:.0} fichiers/s ({:.0} entrées/s), arbre {}{errs}",
            fmt::thousands(r.files),
            fmt::thousands(r.dirs),
            fmt::size(r.logical),
            fmt::size(r.on_disk),
            r.files as f64 / secs,
            (r.files + r.dirs) as f64 / secs,
            fmt::size(r.mem),
        );
        if o.profile {
            let p = &r.phases;
            let sec = |ns: u64| format!("{:.2}", ns as f64 / 1e9);
            println!(
                "phases (somme des workers) : ouverture {} s · lecture {} s · décodage {} s · envoi {} s · {} appels WOF · consommateur : ingestion {:.2} s, attente {:.2} s",
                sec(p.open),
                sec(p.query),
                sec(p.parse),
                sec(p.send),
                p.wof_calls,
                r.ingest_us as f64 / 1e6,
                r.wait_us as f64 / 1e6
            );
        }
    }
    if r.root_denied {
        3
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn bench_args() {
        let o = |path: &str, json, profile| Ok(BenchOpts { path: path.into(), json, profile });
        assert_eq!(super::bench_args(&v(&["C:\\x"])), o("C:\\x", false, false));
        assert_eq!(super::bench_args(&v(&["C:\\x", "--json"])), o("C:\\x", true, false));
        assert_eq!(super::bench_args(&v(&["C:\\x", "--profile", "--json"])), o("C:\\x", true, true));
        assert_eq!(super::bench_args(&v(&["C:\\x", "--foo"])), Err("option inconnue : --foo".into()));
    }

    #[test]
    fn json_str_escapes() {
        assert_eq!(json_str(r#"C:\a"b"#), r#""C:\\a\"b""#);
        assert_eq!(json_str("a\tb"), r#""a\u0009b""#);
        assert_eq!(json_str("é"), "\"é\"");
    }

    #[test]
    fn bench_json_line() {
        let r = Report {
            files: 244_167,
            dirs: 216_278,
            logical: 71_081_234_567,
            on_disk: 56_012_345_678,
            secs: 4.110_26,
            errors: 12,
            root_denied: false,
            mem: 87_654_321,
            threads: 32,
            phases: scan::Phases { open: 1_500_000, query: 2_000_999, parse: 300_000, send: 40_000, wof_calls: 5, dirs_read: 21 },
            ingest_us: 77,
            wait_us: 88,
        };
        let base = format!(
            r#"{{"version":"{}","path":"C:\\Windows","files":244167,"dirs":216278,"logical":71081234567,"on_disk":56012345678,"secs":4.1103,"errors":12,"root_denied":false,"mem":87654321,"threads":32"#,
            env!("CARGO_PKG_VERSION")
        );
        assert_eq!(bench_json("C:\\Windows", &r, false), format!("{base}}}"));
        assert_eq!(
            bench_json("C:\\Windows", &r, true),
            format!(
                r#"{base},"phases":{{"open_us":1500,"query_us":2000,"parse_us":300,"send_us":40,"wof_calls":5,"dirs_read":21,"ingest_us":77,"wait_us":88}}}}"#
            )
        );
        assert!(!bench_json("a\nb", &r, true).contains('\n'));
    }
}
