use std::mem::size_of;
use windows::Win32::Devices::Display::{
    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
    DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO,
    DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME, DisplayConfigGetDeviceInfo,
    GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS, QueryDisplayConfig,
};
use windows::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, LUID};

type DeviceKey = (i32, u32, u32);

struct TargetName {
    connection: String,
    target: DeviceKey,
    name: Option<String>,
}

#[derive(Default)]
pub(crate) struct MonitorNames {
    targets: Vec<TargetName>,
}

impl MonitorNames {
    pub fn query() -> Self {
        let mut result = Self::default();
        // Query each source once, including failures, so a failed clone lookup cannot
        // turn one of its other targets into an apparently unique monitor.
        let mut sources: Vec<(DeviceKey, Option<String>)> = Vec::new();
        for path in active_paths() {
            let source_key = device_key(path.sourceInfo.adapterId, path.sourceInfo.id);
            let source_index = match sources.iter().position(|(key, _)| *key == source_key) {
                Some(index) => index,
                None => {
                    sources.push((source_key, source_name(&path)));
                    sources.len() - 1
                }
            };
            let Some(connection) = sources[source_index].1.as_ref() else {
                continue;
            };
            let target = device_key(path.targetInfo.adapterId, path.targetInfo.id);
            if result.targets.iter().any(|entry| {
                entry.target == target && entry.connection.eq_ignore_ascii_case(connection)
            }) {
                continue;
            }
            result.targets.push(TargetName {
                connection: connection.clone(),
                target,
                name: target_name(&path),
            });
        }
        result
    }

    pub fn for_logical(&self, connection: &str, index: usize) -> String {
        self.friendly_name(connection)
            .map_or_else(|| format!("显示器 {}", index + 1), str::to_owned)
    }

    pub fn for_physical(
        &self,
        connection: &str,
        description: &str,
        logical_index: usize,
        physical_index: usize,
        physical_count: usize,
    ) -> String {
        // DisplayConfig target order is not the physical DDC handle order. Only
        // apply a target name when both APIs describe an unambiguous 1:1 mapping.
        if let Some(name) = self
            .friendly_name(connection)
            .filter(|_| physical_count == 1)
        {
            return name.to_owned();
        }
        if usable_name(description) {
            return description.to_owned();
        }
        if physical_count > 1 {
            format!("显示器 {}.{}", logical_index + 1, physical_index + 1)
        } else {
            format!("显示器 {}", logical_index + 1)
        }
    }

    fn friendly_name(&self, connection: &str) -> Option<&str> {
        let mut matching = self
            .targets
            .iter()
            .filter(|entry| entry.connection.eq_ignore_ascii_case(connection));
        let target = matching.next()?;
        if matching.next().is_some() {
            return None;
        }
        target.name.as_deref().filter(|name| usable_name(name))
    }
}

fn device_key(adapter: LUID, id: u32) -> DeviceKey {
    (adapter.HighPart, adapter.LowPart, id)
}

fn active_paths() -> Vec<DISPLAYCONFIG_PATH_INFO> {
    for _ in 0..3 {
        let (mut path_count, mut mode_count) = (0, 0);
        // SAFETY: Both count outputs are live. Only currently active paths are read.
        let result = unsafe {
            GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count)
        };
        if result != ERROR_SUCCESS
            || !(1..=4096).contains(&path_count)
            || !(1..=4096).contains(&mode_count)
        {
            return Vec::new();
        }
        let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
        // SAFETY: Both arrays have the API-reported capacities. Active-path queries
        // require a null topology pointer; mode indexes are not used after this call.
        let result = unsafe {
            QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut path_count,
                paths.as_mut_ptr(),
                &mut mode_count,
                modes.as_mut_ptr(),
                None,
            )
        };
        if result == ERROR_INSUFFICIENT_BUFFER {
            continue;
        }
        if result != ERROR_SUCCESS
            || path_count as usize > paths.len()
            || mode_count as usize > modes.len()
        {
            return Vec::new();
        }
        paths.truncate(path_count as usize);
        return paths;
    }
    // Rapid topology changes or API failures only affect names, never DDC control.
    Vec::new()
}

fn source_name(path: &DISPLAYCONFIG_PATH_INFO) -> Option<String> {
    let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
        header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
            size: size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
            adapterId: path.sourceInfo.adapterId,
            id: path.sourceInfo.id,
        },
        ..Default::default()
    };
    // SAFETY: The header starts a correctly sized source-name request buffer.
    if unsafe { DisplayConfigGetDeviceInfo(&mut source.header) } != ERROR_SUCCESS.0 as i32 {
        return None;
    }
    let name = from_wide(&source.viewGdiDeviceName);
    (!name.is_empty()).then_some(name)
}

fn target_name(path: &DISPLAYCONFIG_PATH_INFO) -> Option<String> {
    let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME {
        header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
            size: size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
            adapterId: path.targetInfo.adapterId,
            id: path.targetInfo.id,
        },
        ..Default::default()
    };
    // SAFETY: The header starts a correctly sized target-name request buffer.
    if unsafe { DisplayConfigGetDeviceInfo(&mut target.header) } != ERROR_SUCCESS.0 as i32 {
        return None;
    }
    let name = from_wide(&target.monitorFriendlyDeviceName);
    usable_name(&name).then_some(name)
}

fn usable_name(name: &str) -> bool {
    !name.trim().is_empty()
        && !name.chars().any(char::is_control)
        && ![
            "Generic PnP Monitor",
            "Generic Non-PnP Monitor",
            "通用即插即用监视器",
            "通用非即插即用监视器",
            "通用即插即用显示器",
            "通用非即插即用显示器",
        ]
        .iter()
        .any(|generic| name.eq_ignore_ascii_case(generic))
}

pub(crate) fn from_wide(value: &[u16]) -> String {
    let length = value
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(value.len());
    String::from_utf16_lossy(&value[..length]).trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(connection: &str, id: u32, name: Option<&str>) -> TargetName {
        TargetName {
            connection: connection.into(),
            target: (0, 1, id),
            name: name.map(String::from),
        }
    }

    #[test]
    fn matches_friendly_names_by_gdi_source_instead_of_enumeration_order() {
        let names = MonitorNames {
            targets: vec![
                target(r"\\.\DISPLAY2", 2, Some("DELL U2723QE")),
                target(r"\\.\DISPLAY1", 1, Some("LG ULTRAGEAR")),
            ],
        };
        assert_eq!(
            names.for_physical(r"\\.\display1", "Generic PnP Monitor", 0, 0, 1),
            "LG ULTRAGEAR"
        );
        assert_eq!(
            names.for_physical(r"\\.\DISPLAY2", "Generic PnP Monitor", 1, 0, 1),
            "DELL U2723QE"
        );
    }

    #[test]
    fn a_missing_clone_name_does_not_make_the_remaining_target_unambiguous() {
        let names = MonitorNames {
            targets: vec![
                target(r"\\.\DISPLAY1", 1, Some("Screen A")),
                target(r"\\.\DISPLAY1", 2, None),
            ],
        };
        assert_eq!(
            names.for_physical(r"\\.\DISPLAY1", "Generic PnP Monitor", 0, 0, 1),
            "显示器 1"
        );
    }

    #[test]
    fn multiple_physical_handles_do_not_borrow_a_single_target_name() {
        let names = MonitorNames {
            targets: vec![target(r"\\.\DISPLAY1", 1, Some("Screen A"))],
        };
        assert_eq!(
            names.for_physical(r"\\.\DISPLAY1", "Generic PnP Monitor", 0, 1, 2),
            "显示器 1.2"
        );
    }

    #[test]
    fn preserves_a_specific_ddc_description_when_display_config_is_unavailable() {
        assert_eq!(
            MonitorNames::default().for_physical(r"\\.\DISPLAY1", "BenQ PD2705U", 0, 0, 1),
            "BenQ PD2705U"
        );
    }

    #[test]
    fn replaces_generic_localized_or_empty_descriptions_with_numbered_labels() {
        for description in [
            "Generic PnP Monitor",
            "Generic Non-PnP Monitor",
            "通用即插即用监视器",
            " ",
        ] {
            assert_eq!(
                MonitorNames::default().for_physical(r"\\.\DISPLAY2", description, 1, 0, 1),
                "显示器 2"
            );
        }
    }

    #[test]
    fn names_unavailable_logical_monitors_without_opening_ddc_handles() {
        let names = MonitorNames {
            targets: vec![target(r"\\.\DISPLAY1", 1, Some("联想 显示器"))],
        };
        assert_eq!(names.for_logical(r"\\.\DISPLAY1", 0), "联想 显示器");
        assert_eq!(names.for_logical(r"\\.\DISPLAY2", 1), "显示器 2");
    }

    #[test]
    fn invalid_friendly_names_fall_back_to_a_specific_ddc_description() {
        let names = MonitorNames {
            targets: vec![target(r"\\.\DISPLAY1", 1, Some("Generic PnP Monitor"))],
        };
        assert_eq!(
            names.for_physical(r"\\.\DISPLAY1", "ASUS VG27A", 0, 0, 1),
            "ASUS VG27A"
        );
        assert!(!usable_name("Screen\nA"));
    }
}
