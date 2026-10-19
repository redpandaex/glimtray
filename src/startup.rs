use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};
use windows::core::{PCWSTR, w};

pub struct Startup {
    key: Vec<u16>,
    command: Vec<u16>,
}

impl Startup {
    pub fn current() -> Result<Self, String> {
        let executable = std::env::current_exe().map_err(|error| error.to_string())?;
        Ok(Self {
            key: "Software\\Microsoft\\Windows\\CurrentVersion\\Run\0"
                .encode_utf16()
                .collect(),
            command: command(&executable)?,
        })
    }

    pub fn enabled(&self) -> Result<bool, String> {
        // Run commands are limited to 260 UTF-16 code units, plus the terminator.
        let mut data = [0u16; 261];
        let mut size = std::mem::size_of_val(&data) as u32;
        // SAFETY: Both names are terminated; the output buffer and byte count agree.
        let result = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                PCWSTR(self.key.as_ptr()),
                w!("GlimTray"),
                RRF_RT_REG_SZ,
                None,
                Some(data.as_mut_ptr().cast()),
                Some(&mut size),
            )
        };
        if result == ERROR_FILE_NOT_FOUND || result == ERROR_PATH_NOT_FOUND {
            return Ok(false);
        }
        result.ok().map_err(|error| error.to_string())?;
        Ok(data.get(..size as usize / 2) == Some(self.command.as_slice()))
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        // SAFETY: Only our named value is changed. Strings are terminated and the byte
        // count includes the UTF-16 terminator. RegSetKeyValueW creates a missing subkey.
        let result = unsafe {
            if enabled {
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    PCWSTR(self.key.as_ptr()),
                    w!("GlimTray"),
                    REG_SZ.0,
                    Some(self.command.as_ptr().cast()),
                    (self.command.len() * 2) as u32,
                )
            } else {
                RegDeleteKeyValueW(HKEY_CURRENT_USER, PCWSTR(self.key.as_ptr()), w!("GlimTray"))
            }
        };
        if !enabled && (result == ERROR_FILE_NOT_FOUND || result == ERROR_PATH_NOT_FOUND) {
            return Ok(());
        }
        if result != ERROR_SUCCESS {
            return Err(result.to_hresult().message().to_string());
        }
        if self.enabled()? != enabled {
            return Err("Windows 未保存自启设置，请重试。".into());
        }
        Ok(())
    }
}

fn command(executable: &Path) -> Result<Vec<u16>, String> {
    let path: Vec<u16> = executable.as_os_str().encode_wide().collect();
    if !executable.is_absolute() || path.contains(&0) || path.contains(&34) {
        return Err("程序路径无效，无法设置自启。".into());
    }
    if path.len() + 2 > 260 {
        return Err("程序路径过长，请移到较短的目录后设置自启。".into());
    }
    Ok(std::iter::once(34).chain(path).chain([34, 0]).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_command_quotes_paths_with_spaces_and_chinese() {
        let actual = command(Path::new(r"D:\应用程序\Glim Tray\glimtray.exe")).unwrap();
        let expected: Vec<u16> = "\"D:\\应用程序\\Glim Tray\\glimtray.exe\"\0"
            .encode_utf16()
            .collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn invalid_or_overlong_startup_paths_are_rejected() {
        for path in [
            "glimtray.exe".to_string(),
            "D:\\bad\"path\\glimtray.exe".to_string(),
            "D:\\bad\0path\\glimtray.exe".to_string(),
            format!("D:\\{}\\glimtray.exe", "a".repeat(260)),
        ] {
            assert!(command(Path::new(&path)).is_err(), "accepted {path:?}");
        }
    }

    #[test]
    #[ignore = "requires write access to a temporary HKCU test key; never changes Windows startup"]
    fn registry_round_trip_preserves_other_values_and_allows_repeated_disable() {
        use windows::Win32::System::Registry::RegDeleteTreeW;
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let key: Vec<u16> = format!("Software\\GlimTrayTests-{}-{unique}\0", std::process::id())
            .encode_utf16()
            .collect();
        struct Cleanup(Vec<u16>);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                // SAFETY: This unique test-only key is owned by this test, never the Run key.
                let _ = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(self.0.as_ptr())) };
            }
        }
        let _cleanup = Cleanup(key.clone());
        let startup = Startup {
            key,
            command: command(Path::new(r"D:\应用程序\Glim Tray\glimtray.exe")).unwrap(),
        };
        assert!(!startup.enabled().unwrap());
        startup.set_enabled(false).unwrap();
        // SAFETY: Seed a different named value under the isolated test key.
        unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                PCWSTR(startup.key.as_ptr()),
                w!("OtherApp"),
                REG_SZ.0,
                Some(startup.command.as_ptr().cast()),
                (startup.command.len() * 2) as u32,
            )
        }
        .ok()
        .unwrap();
        startup.set_enabled(true).unwrap();
        assert!(startup.enabled().unwrap());
        startup.set_enabled(true).unwrap();
        // A stale registration pointing at a different binary is not enabled for this binary.
        let other_binary = Startup {
            key: startup.key.clone(),
            command: command(Path::new(r"D:\elsewhere\glimtray.exe")).unwrap(),
        };
        assert!(!other_binary.enabled().unwrap());
        startup.set_enabled(false).unwrap();
        assert!(!startup.enabled().unwrap());
        startup.set_enabled(false).unwrap();
        let mut other_data = [0u16; 261];
        let mut size = std::mem::size_of_val(&other_data) as u32;
        // SAFETY: Read the neighboring value into a correctly sized UTF-16 buffer.
        let result = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                PCWSTR(startup.key.as_ptr()),
                w!("OtherApp"),
                RRF_RT_REG_SZ,
                None,
                Some(other_data.as_mut_ptr().cast()),
                Some(&mut size),
            )
        };
        assert_eq!(result, ERROR_SUCCESS);
        assert_eq!(&other_data[..size as usize / 2], startup.command.as_slice());
    }
}
