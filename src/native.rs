use crate::display::{BrightnessRange, DisplaySnapshot};
use std::mem::size_of;
use windows::Win32::Devices::Display::{
    DestroyPhysicalMonitor, GetMonitorBrightness, GetNumberOfPhysicalMonitorsFromHMONITOR,
    GetPhysicalMonitorsFromHMONITOR, GetVCPFeatureAndVCPFeatureReply, PHYSICAL_MONITOR,
    SetMonitorBrightness, SetVCPFeature,
};
use windows::Win32::Foundation::{ERROR_SUCCESS, HANDLE, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::core::{BOOL, Error, w};

struct MonitorHandle(HANDLE);

impl Drop for MonitorHandle {
    fn drop(&mut self) {
        // SAFETY: This owns the unique handle returned by the physical monitor API.
        let _ = unsafe { DestroyPhysicalMonitor(self.0) };
    }
}

#[derive(Clone, Copy)]
enum BrightnessMethod {
    HighLevel,
    Vcp,
}

struct Device {
    handle: Option<MonitorHandle>,
    snapshot: DisplaySnapshot,
    range: Option<BrightnessRange>,
    method: Option<BrightnessMethod>,
}

impl Device {
    fn unavailable(id: String, name: String, connection: String, message: String) -> Self {
        Self {
            handle: None,
            snapshot: DisplaySnapshot {
                id,
                name,
                connection,
                brightness: None,
                message,
            },
            range: None,
            method: None,
        }
    }

    fn read(&mut self) -> Result<(), String> {
        let handle = self
            .handle
            .as_ref()
            .ok_or("当前连接未提供显示器控制接口。")?
            .0;
        let (mut minimum, mut current, mut maximum) = (0, 0, 0);
        // SAFETY: The handle is owned by this device and all output pointers are valid.
        let high_ok =
            unsafe { GetMonitorBrightness(handle, &mut minimum, &mut current, &mut maximum) } != 0;
        let high_range = high_ok
            .then(|| BrightnessRange::new(minimum, current, maximum))
            .flatten();
        let (range, method) = if let Some(range) = high_range {
            (range, BrightnessMethod::HighLevel)
        } else {
            // Avoid capability-string probes; query only the standard brightness feature.
            // SAFETY: The physical handle and both output pointers remain valid.
            let low_ok = unsafe {
                GetVCPFeatureAndVCPFeatureReply(
                    handle,
                    0x10,
                    None,
                    &mut current,
                    Some(&mut maximum),
                )
            } != 0;
            if !low_ok {
                return Err(format!(
                    "无法读取亮度，请检查显示器菜单中的 DDC/CI 设置。（{}）",
                    Error::from_thread()
                ));
            }
            let range =
                BrightnessRange::new(0, current, maximum).ok_or("显示器返回了无效的亮度范围。")?;
            (range, BrightnessMethod::Vcp)
        };
        self.range = Some(range);
        self.method = Some(method);
        self.snapshot.brightness = Some(range.percent());
        self.snapshot.message = "DDC/CI".into();
        Ok(())
    }

    fn set(&mut self, percent: u8) -> Result<DisplaySnapshot, String> {
        if percent > 100 {
            return Err("亮度必须在 0–100% 之间。".into());
        }
        let handle = self.handle.as_ref().ok_or("当前显示器不支持亮度控制。")?.0;
        let range = self.range.ok_or("请刷新并读取亮度后重试。")?;
        let method = self.method.ok_or("当前显示器没有可用的亮度控制方式。")?;
        let raw = range.raw_from_percent(percent);
        // SAFETY: The handle is live, the value was bounded to its previously read range.
        let success = unsafe {
            match method {
                BrightnessMethod::HighLevel => SetMonitorBrightness(handle, raw),
                BrightnessMethod::Vcp => SetVCPFeature(handle, 0x10, raw),
            }
        } != 0;
        if !success {
            return Err(format!(
                "亮度设置失败，请刷新后重试。（{}）",
                Error::from_thread()
            ));
        }
        self.read()
            .map_err(|error| format!("已发送亮度指令，但无法确认结果。{error}"))?;
        Ok(self.snapshot.clone())
    }
}

#[derive(Default)]
pub struct NativeBackend {
    devices: Vec<Device>,
}

impl NativeBackend {
    pub fn refresh(&mut self, epoch: u64) -> Result<Vec<DisplaySnapshot>, String> {
        self.devices.clear();
        let mut logical = Vec::<HMONITOR>::new();
        // SAFETY: The callback is synchronous and receives a valid pointer to this vector.
        unsafe {
            EnumDisplayMonitors(
                None,
                None,
                Some(collect_monitor),
                LPARAM(&mut logical as *mut _ as isize),
            )
            .ok()
            .map_err(|error| format!("无法枚举显示器：{error}"))?;
        }
        for (logical_index, monitor) in logical.into_iter().enumerate() {
            let mut info = MONITORINFOEXW::default();
            info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
            // SAFETY: MONITORINFOEXW starts with MONITORINFO and cbSize describes the full buffer.
            let info_ok = unsafe { GetMonitorInfoW(monitor, &mut info.monitorInfo) }.as_bool();
            let connection = if info_ok {
                from_wide(&info.szDevice)
            } else {
                format!("DISPLAY{}", logical_index + 1)
            };
            let mut count = 0;
            // SAFETY: monitor came from enumeration and count is a valid output pointer.
            let count_result =
                unsafe { GetNumberOfPhysicalMonitorsFromHMONITOR(monitor, &mut count) };
            if count_result.is_err() || count == 0 || count > 64 {
                self.devices.push(Device::unavailable(
                    format!("{epoch}:{logical_index}:unavailable"),
                    format!("显示器 {}", logical_index + 1),
                    connection,
                    "当前连接未提供 DDC/CI 控制接口；内置屏亮度尚未接入。".into(),
                ));
                continue;
            }
            let mut physical = vec![PHYSICAL_MONITOR::default(); count as usize];
            // SAFETY: The output slice has exactly the count returned by the API.
            let result = unsafe { GetPhysicalMonitorsFromHMONITOR(monitor, &mut physical) };
            // Adopt every returned handle before inspecting the result, so partial output is freed.
            let owned: Vec<_> = physical
                .into_iter()
                .enumerate()
                .filter_map(|(index, physical)| {
                    let handle = physical.hPhysicalMonitor;
                    let description = physical.szPhysicalMonitorDescription;
                    should_adopt_handle(result.is_ok(), handle)
                        .then(|| (index, MonitorHandle(handle), from_wide(&description)))
                })
                .collect();
            if result.is_err() || owned.is_empty() {
                self.devices.push(Device::unavailable(
                    format!("{epoch}:{logical_index}:unavailable"),
                    format!("显示器 {}", logical_index + 1),
                    connection,
                    "无法打开显示器控制接口，请检查连接后刷新。".into(),
                ));
                continue;
            }
            for (physical_index, handle, name) in owned {
                let mut device = Device::unavailable(
                    format!("{epoch}:{logical_index}:{physical_index}"),
                    if name.is_empty() {
                        format!("显示器 {}", logical_index + 1)
                    } else {
                        name
                    },
                    connection.clone(),
                    String::new(),
                );
                device.handle = Some(handle);
                if let Err(message) = device.read() {
                    device.snapshot.message = message;
                }
                self.devices.push(device);
            }
        }
        Ok(self
            .devices
            .iter()
            .map(|device| device.snapshot.clone())
            .collect())
    }

    pub fn set(&mut self, id: &str, percent: u8) -> Result<DisplaySnapshot, String> {
        self.devices
            .iter_mut()
            .find(|device| device.snapshot.id == id)
            .ok_or("设备已变更，请刷新显示器列表后重试。")?
            .set(percent)
    }
}

unsafe extern "system" fn collect_monitor(
    monitor: HMONITOR,
    _dc: HDC,
    _rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    // SAFETY: EnumDisplayMonitors runs synchronously; data points to its caller's live vector.
    unsafe { (&mut *(data.0 as *mut Vec<HMONITOR>)).push(monitor) };
    BOOL(1)
}

fn from_wide(value: &[u16]) -> String {
    let length = value
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(value.len());
    String::from_utf16_lossy(&value[..length]).trim().to_owned()
}

fn should_adopt_handle(enumeration_succeeded: bool, handle: HANDLE) -> bool {
    // Physical monitor handles are opaque tokens; a successful API call can return zero.
    // Only reject unwritten/invalid output when enumeration itself failed.
    enumeration_succeeded || !handle.is_invalid()
}

pub fn dark_taskbar() -> bool {
    let mut light = 1u32;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: Read-only DWORD query into a correctly sized buffer. No key is modified.
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut light as *mut u32).cast()),
            Some(&mut size),
        )
    };
    result == ERROR_SUCCESS && light == 0
}

pub fn finish_startup_feedback() {
    use windows::Win32::Foundation::{HWND, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostMessageW, PostQuitMessage,
        RegisterWindowMessageW,
    };

    // Windows ends GUI startup feedback on GetMessage. Winit only uses PeekMessage,
    // and the hidden DDC worker has no event loop. Consume only our own thread message.
    // SAFETY: The queue and message buffer belong to this thread. A successful post
    // precedes the filtered GetMessage, so it does not wait for external input.
    unsafe {
        let ready = RegisterWindowMessageW(w!("GlimTray.StartupReady"));
        if ready == 0 {
            return;
        }
        let mut message = MSG::default();
        let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
        if PostMessageW(None, ready, WPARAM(0), LPARAM(0)).is_err() {
            return;
        }
        if GetMessageW(&mut message, Some(HWND(-1isize as _)), ready, ready).0 == 0 {
            // WM_QUIT bypasses filters; preserve it for the actual event loop.
            PostQuitMessage(message.wParam.0 as i32);
        }
    }
}

pub fn flyout_anchor() -> Option<crate::flyout::Anchor> {
    use crate::flyout::{Anchor, Point, Rect};
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint};
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut cursor = POINT::default();
    // SAFETY: Both APIs are read-only and receive correctly sized, live output buffers.
    unsafe { GetCursorPos(&mut cursor) }.ok()?;
    let monitor = unsafe { MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return None;
    }
    let (mut dpi_x, mut dpi_y) = (96, 96);
    // SAFETY: monitor was resolved from the cursor and the output pointers are valid.
    let _ = unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) };
    let rect = |value: RECT| Rect {
        left: value.left,
        top: value.top,
        right: value.right,
        bottom: value.bottom,
    };
    Some(Anchor {
        cursor: Point {
            x: cursor.x,
            y: cursor.y,
        },
        screen: rect(info.rcMonitor),
        work: rect(info.rcWork),
        scale: (dpi_x.max(1) as f32 / 96.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successful_physical_enumeration_can_return_an_opaque_zero_handle() {
        assert!(should_adopt_handle(true, HANDLE::default()));
    }

    #[test]
    fn unsuccessful_enumeration_does_not_adopt_unwritten_zero_output() {
        assert!(!should_adopt_handle(false, HANDLE::default()));
    }
}
