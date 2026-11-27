//! The date shown on a library card. It is the clip file's last-modified date (what Explorer
//! shows as "Date modified"), in the PC's local time. It is not a recording or creation date.

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// Civil date (year, month 1-12, day 1-31) of a day count since 1970-01-01 (proleptic Gregorian).
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// "27 Nov 2026" for Unix seconds shifted by the local UTC offset in minutes (east is positive).
pub fn format_date(unix_secs: i64, utc_offset_min: i32) -> String {
    let local = unix_secs + utc_offset_min as i64 * 60;
    let (y, m, d) = civil_from_days(local.div_euclid(86_400));
    format!("{d} {} {y}", MONTHS[(m - 1) as usize])
}

#[cfg(windows)]
fn utc_offset_min() -> i32 {
    #[repr(C)]
    struct Tzi {
        bias: i32,
        std_name: [u16; 32],
        std_date: [u16; 8],
        std_bias: i32,
        dst_name: [u16; 32],
        dst_date: [u16; 8],
        dst_bias: i32,
    }
    extern "system" {
        fn GetTimeZoneInformation(info: *mut Tzi) -> u32;
    }
    unsafe {
        let mut t: Tzi = std::mem::zeroed();
        let kind = GetTimeZoneInformation(&mut t);
        // Windows reports UTC minus local, so the offset east of UTC is the negative.
        -(t.bias + if kind == 2 { t.dst_bias } else { t.std_bias })
    }
}
#[cfg(not(windows))]
fn utc_offset_min() -> i32 {
    0
}

/// Last-modified date of a file as text, or empty when it cannot be read.
pub fn modified_label(path: &std::path::Path) -> String {
    let Ok(modified) = std::fs::metadata(path).and_then(|m| m.modified()) else { return String::new() };
    let secs = match modified.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    };
    format_date(secs, utc_offset_min())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn known_dates() {
        assert_eq!(format_date(0, 0), "1 Jan 1970");
        assert_eq!(format_date(1_795_780_800, 0), "27 Nov 2026");
        assert_eq!(format_date(951_782_400, 0), "29 Feb 2000");
        assert_eq!(format_date(1_709_164_800, 0), "29 Feb 2024");
    }
    #[test]
    fn local_offset_can_change_the_day() {
        // 2026-11-27 23:30 UTC is already the 28th at UTC+8 and still the 27th at UTC-5.
        let t = 1_795_780_800 + 11 * 3600 + 30 * 60;
        assert_eq!(format_date(t, 480), "28 Nov 2026");
        assert_eq!(format_date(t, -300), "27 Nov 2026");
    }
}
