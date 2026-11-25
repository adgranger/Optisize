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

/// Date en heure locale : « aujourd’hui », « 12 nov. » ou « 9 sept. 2023 ». `secs` et `now` en secondes Unix UTC, `0` = inconnue.
pub fn date(secs: u32, now: u64) -> String {
    if secs == 0 {
        return String::new();
    }
    date_local(local(secs as i64), local(now as i64))
}

/// Logique d'affichage pure, sur des secondes « Unix locales ».
fn date_local(local: i64, local_now: i64) -> String {
    let (day, now_day) = (local.div_euclid(86_400), local_now.div_euclid(86_400));
    if day == now_day {
        return "aujourd’hui".into();
    }
    let (y, m, d) = civil(day);
    let (ny, ..) = civil(now_day);
    const M: [&str; 12] =
        ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."];
    if y == ny {
        format!("{d} {}", M[m as usize - 1])
    } else {
        format!("{d} {} {y}", M[m as usize - 1])
    }
}

#[cfg(windows)]
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct FileTime {
    low: u32,
    high: u32,
}

#[cfg(windows)]
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct SystemTime {
    year: u16,
    month: u16,
    day_of_week: u16,
    day: u16,
    hour: u16,
    minute: u16,
    second: u16,
    millis: u16,
}

#[cfg(windows)]
#[repr(C)]
#[derive(Clone, Copy)]
struct TimeZoneInfo {
    bias: i32,
    standard_name: [u16; 32],
    standard_date: SystemTime,
    standard_bias: i32,
    daylight_name: [u16; 32],
    daylight_date: SystemTime,
    daylight_bias: i32,
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn FileTimeToSystemTime(ft: *const FileTime, st: *mut SystemTime) -> i32;
    fn SystemTimeToTzSpecificLocalTime(tz: *const TimeZoneInfo, utc: *const SystemTime, local: *mut SystemTime) -> i32;
    fn SystemTimeToFileTime(st: *const SystemTime, ft: *mut FileTime) -> i32;
}

/// Secondes Unix UTC -> secondes « Unix locales » (décalées du fuseau actif, heure d'été de la date comprise).
/// shortcut: règle d'heure d'été courante appliquée à toutes les années, passer à SystemTimeToTzSpecificLocalTimeEx + GetDynamicTimeZoneInformation si un utilisateur le signale.
fn local(utc: i64) -> i64 {
    #[cfg(windows)]
    return local_with(std::ptr::null(), utc);
    #[cfg(not(windows))]
    utc
}

/// `tz` nul = fuseau actif de la machine ; non nul pour les tests. Repli sur `utc` si un appel échoue.
#[cfg(windows)]
fn local_with(tz: *const TimeZoneInfo, utc: i64) -> i64 {
    const EPOCH: i64 = 11_644_473_600;
    let ticks = ((utc + EPOCH) * 10_000_000) as u64;
    let ft = FileTime { low: ticks as u32, high: (ticks >> 32) as u32 };
    let (mut st, mut lst, mut out) = (SystemTime::default(), SystemTime::default(), FileTime::default());
    // SAFETY: pointeurs valides vers des structures repr(C) locales ; `tz` est nul ou valide.
    let ok = unsafe {
        FileTimeToSystemTime(&ft, &mut st) != 0
            && SystemTimeToTzSpecificLocalTime(tz, &st, &mut lst) != 0
            && SystemTimeToFileTime(&lst, &mut out) != 0
    };
    if !ok {
        return utc;
    }
    (((out.high as u64) << 32 | out.low as u64) / 10_000_000) as i64 - EPOCH
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

    const T_2024_07_01: i64 = 1_719_792_000;

    #[test]
    fn date_local_today() {
        assert_eq!(date_local(T_2024_07_01 + 36_000, T_2024_07_01 + 39_600), "aujourd’hui");
    }

    #[test]
    fn date_local_same_year() {
        assert_eq!(date_local(1_731_412_800, 1_733_011_200), "12 nov.");
    }

    #[test]
    fn date_local_other_year() {
        assert_eq!(date_local(1_694_260_800, 1_733_011_200), "9 sept. 2023");
    }

    #[test]
    fn date_local_future() {
        assert_eq!(date_local(1_930_305_600, 1_733_011_200), "3 mars 2031");
        assert_eq!(date_local(1_734_652_800, 1_733_011_200), "20 déc.");
    }

    #[test]
    fn date_local_negative() {
        assert_eq!(date_local(-17_999, 0), "31 déc. 1969");
    }

    #[test]
    fn date_zero_is_empty() {
        assert_eq!(date(0, 0), "");
        assert_eq!(date(0, 1_700_000_000), "");
    }

    #[cfg(windows)]
    fn paris() -> TimeZoneInfo {
        let z = |month, hour| SystemTime { month, day_of_week: 0, day: 5, hour, ..Default::default() };
        TimeZoneInfo {
            bias: -60,
            standard_name: [0; 32],
            standard_date: z(10, 3),
            standard_bias: 0,
            daylight_name: [0; 32],
            daylight_date: z(3, 2),
            daylight_bias: -60,
        }
    }

    #[cfg(windows)]
    fn india() -> TimeZoneInfo {
        TimeZoneInfo {
            bias: -330,
            standard_name: [0; 32],
            standard_date: SystemTime::default(),
            standard_bias: 0,
            daylight_name: [0; 32],
            daylight_date: SystemTime::default(),
            daylight_bias: 0,
        }
    }

    #[cfg(windows)]
    #[test]
    fn tz_struct_sizes() {
        assert_eq!(std::mem::size_of::<TimeZoneInfo>(), 172);
        assert_eq!(std::mem::size_of::<SystemTime>(), 16);
    }

    #[cfg(windows)]
    #[test]
    fn paris_offsets() {
        let p = paris();
        let (w, s) = (1_705_320_000, 1_721_044_800); // 2024-01-15 / 2024-07-15 12:00 UTC
        assert_eq!(local_with(&p, w), w + 3600);
        assert_eq!(local_with(&p, s), s + 7200);
    }

    #[cfg(windows)]
    #[test]
    fn paris_dst_switch() {
        let p = paris();
        assert_eq!(local_with(&p, 1_711_846_799), 1_711_846_799 + 3600);
        assert_eq!(local_with(&p, 1_711_846_800), 1_711_846_800 + 7200);
    }

    #[cfg(windows)]
    #[test]
    fn paris_midnight_summer() {
        let p = paris();
        let (f, now) = (T_2024_07_01 - 5400, T_2024_07_01 + 36_000); // 22:30 UTC le 30 juin
        assert_eq!(date_local(local_with(&p, f), local_with(&p, now)), "aujourd’hui");
    }

    #[cfg(windows)]
    #[test]
    fn paris_new_year() {
        let p = paris();
        let (f, now) = (1_704_067_200 - 1800, 1_709_251_200); // 2023-12-31 23:30 UTC ; 2024-03-01
        assert_eq!(date_local(local_with(&p, f), local_with(&p, now)), "1 janv.");
    }

    #[cfg(windows)]
    #[test]
    fn india_half_hour() {
        let (p, f) = (india(), 1_704_067_200 + 72_000); // 2024-01-01 20:00 UTC
        assert_eq!(local_with(&p, f) - f, 19_800);
        assert_eq!(date_local(local_with(&p, f), local_with(&p, f + 86_400 * 30)), "2 janv.");
    }

    #[cfg(windows)]
    #[test]
    fn local_active_zone_is_sane() {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
        let off = local(now) - now;
        assert_eq!(off % 900, 0);
        assert!(off.abs() <= 14 * 3600);
    }
}
