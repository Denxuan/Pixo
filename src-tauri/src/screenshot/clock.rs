pub fn local_stamp() -> String {
    #[cfg(unix)]
    {
        unix_stamp()
    }
    #[cfg(windows)]
    {
        windows_stamp()
    }
    #[cfg(not(any(unix, windows)))]
    {
        "00000000_000000".into()
    }
}

#[cfg(unix)]
fn unix_stamp() -> String {
    #[repr(C)]
    struct Tm {
        sec: i32,
        min: i32,
        hour: i32,
        mday: i32,
        mon: i32,
        year: i32,
        wday: i32,
        yday: i32,
        isdst: i32,
        gmtoff: i64,
        zone: *const u8,
    }
    unsafe extern "C" {
        fn time(tloc: *mut i64) -> i64;
        fn localtime_r(clock: *const i64, result: *mut Tm) -> *mut Tm;
    }
    unsafe {
        let mut now = 0i64;
        time(&mut now);
        let mut parts = std::mem::zeroed::<Tm>();
        if localtime_r(&now, &mut parts).is_null() {
            return "00000000_000000".into();
        }
        format!(
            "{:04}{:02}{:02}_{:02}{:02}{:02}",
            parts.year + 1900,
            parts.mon + 1,
            parts.mday,
            parts.hour,
            parts.min,
            parts.sec
        )
    }
}

#[cfg(windows)]
fn windows_stamp() -> String {
    use windows::Win32::System::SystemInformation::GetLocalTime;
    let time = unsafe { GetLocalTime() };
    format!(
        "{:04}{:02}{:02}_{:02}{:02}{:02}",
        time.wYear, time.wMonth, time.wDay, time.wHour, time.wMinute, time.wSecond
    )
}

pub fn unique_file(directory: &std::path::Path, extension: &str) -> std::path::PathBuf {
    let stamp = local_stamp();
    let mut path = directory.join(format!("Pixo_{stamp}.{extension}"));
    let mut suffix = 2u32;
    while path.exists() && suffix < 1000 {
        path = directory.join(format!("Pixo_{stamp}-{suffix}.{extension}"));
        suffix += 1;
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamp_has_the_filename_shape() {
        let stamp = local_stamp();
        assert_eq!(stamp.len(), 15);
        assert_eq!(stamp.as_bytes()[8], b'_');
    }

    #[test]
    fn unique_file_adds_a_numeric_suffix() {
        let directory = std::env::temp_dir().join(format!("pixo-shot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let first = unique_file(&directory, "png");
        std::fs::write(&first, b"a").unwrap();
        let second = unique_file(&directory, "png");
        assert_ne!(first, second);
        assert!(second.file_name().unwrap().to_string_lossy().contains('-'));
        let _ = std::fs::remove_dir_all(&directory);
    }
}
