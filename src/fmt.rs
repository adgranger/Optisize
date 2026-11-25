//! Formatage à la française (virgule décimale, espaces pour les milliers).

pub fn size(b: u64) -> String {
    const U: [&str; 5] = ["o", "Ko", "Mo", "Go", "To"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < 4 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b} o")
    } else {
        format!("{v:.1} {}", U[i]).replace('.', ",")
    }
}

pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

pub fn percent(p: f64) -> String {
    format!("{p:.1} %").replace('.', ",")
}

/// Date (UTC) : « aujourd'hui », « 12 nov. » ou « 9 sept. 2023 ». `now` en secondes Unix.
pub fn date(secs: u32, now: u64) -> String {
    if secs == 0 {
        return String::new();
    }
    let (y, m, d) = civil(secs as i64 / 86_400);
    let (ny, ..) = civil(now as i64 / 86_400);
    if now / 86_400 == secs as u64 / 86_400 {
        return "aujourd’hui".into();
    }
    const M: [&str; 12] =
        ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."];
    if y == ny {
        format!("{d} {}", M[m as usize - 1])
    } else {
        format!("{d} {} {y}", M[m as usize - 1])
    }
}

/// Jours depuis 1970-01-01 -> (année, mois, jour) (algorithme de H. Hinnant).
fn civil(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    ((yoe + era * 400) + (m <= 2) as i64, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(thousands(1_284_903), "1 284 903");
        assert_eq!(size(1536), "1,5 Ko");
        assert_eq!(size(12), "12 o");
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(19_723), (2024, 1, 1));
    }
}
