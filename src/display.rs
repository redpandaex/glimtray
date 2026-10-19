#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplaySnapshot {
    pub id: String,
    pub name: String,
    pub connection: String,
    pub brightness: Option<u8>,
    pub message: String,
}

/// Format snapshots as the stable pretty JSON used by the `--list` diagnostic.
pub fn json_array(displays: &[DisplaySnapshot]) -> String {
    if displays.is_empty() {
        return "[]".into();
    }
    let mut json = String::from("[\n");
    for (index, display) in displays.iter().enumerate() {
        let brightness = display
            .brightness
            .map(|value| value.to_string())
            .unwrap_or_else(|| "null".into());
        json.push_str("  {\n");
        json.push_str(&format!("    \"id\": {},\n", json_string(&display.id)));
        json.push_str(&format!("    \"name\": {},\n", json_string(&display.name)));
        json.push_str(&format!(
            "    \"connection\": {},\n",
            json_string(&display.connection)
        ));
        json.push_str(&format!("    \"brightness\": {brightness},\n"));
        json.push_str(&format!(
            "    \"message\": {}\n",
            json_string(&display.message)
        ));
        json.push_str("  }");
        if index + 1 < displays.len() {
            json.push(',');
        }
        json.push('\n');
    }
    json.push(']');
    json
}

fn json_string(value: &str) -> String {
    let mut json = String::with_capacity(value.len() + 2);
    json.push('"');
    for character in value.chars() {
        match character {
            '"' => json.push_str("\\\""),
            '\\' => json.push_str("\\\\"),
            '\n' => json.push_str("\\n"),
            '\r' => json.push_str("\\r"),
            '\t' => json.push_str("\\t"),
            control if (control as u32) < 0x20 => {
                json.push_str(&format!("\\u{:04x}", control as u32));
            }
            other => json.push(other),
        }
    }
    json.push('"');
    json
}

#[derive(Clone, Copy, Debug)]
pub struct BrightnessRange {
    minimum: u32,
    current: u32,
    maximum: u32,
}

impl BrightnessRange {
    pub fn new(minimum: u32, current: u32, maximum: u32) -> Option<Self> {
        (maximum > minimum).then(|| Self {
            minimum,
            current: current.clamp(minimum, maximum),
            maximum,
        })
    }

    pub fn percent(self) -> u8 {
        let span = u64::from(self.maximum - self.minimum);
        ((u64::from(self.current - self.minimum) * 100 + span / 2) / span) as u8
    }

    pub fn raw_from_percent(self, percent: u8) -> u32 {
        let span = u64::from(self.maximum - self.minimum);
        (u64::from(self.minimum) + (span * u64::from(percent.min(100)) + 50) / 100) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_nonzero_minimum_and_nonstandard_maximum() {
        let range = BrightnessRange::new(20, 70, 220).unwrap();
        assert_eq!(range.percent(), 25);
        assert_eq!(range.raw_from_percent(0), 20);
        assert_eq!(range.raw_from_percent(50), 120);
        assert_eq!(range.raw_from_percent(100), 220);
    }

    #[test]
    fn rejects_zero_width_or_reversed_ranges() {
        assert!(BrightnessRange::new(0, 0, 0).is_none());
        assert!(BrightnessRange::new(50, 50, 50).is_none());
        assert!(BrightnessRange::new(100, 20, 10).is_none());
    }

    #[test]
    fn clamps_malformed_device_values_and_requested_percentages() {
        assert_eq!(BrightnessRange::new(20, 0, 220).unwrap().percent(), 0);
        assert_eq!(BrightnessRange::new(20, 300, 220).unwrap().percent(), 100);
        assert_eq!(
            BrightnessRange::new(20, 70, 220)
                .unwrap()
                .raw_from_percent(255),
            220
        );
    }

    #[test]
    fn handles_u32_limits_without_overflow() {
        let range = BrightnessRange::new(u32::MAX - 100, u32::MAX - 50, u32::MAX).unwrap();
        assert_eq!(range.percent(), 50);
        assert_eq!(range.raw_from_percent(100), u32::MAX);
    }

    #[test]
    fn rounds_small_ranges_to_the_nearest_supported_step() {
        let range = BrightnessRange::new(0, 1, 3).unwrap();
        assert_eq!(range.percent(), 33);
        assert_eq!(range.raw_from_percent(50), 2);
    }

    #[test]
    fn json_array_escapes_text_and_preserves_optional_brightness() {
        let displays = vec![
            DisplaySnapshot {
                id: "1:0:0".into(),
                name: "桌面\t\"显示器\"\\".into(),
                connection: "DISPLAY1".into(),
                brightness: Some(75),
                message: "DDC/CI\n就绪".into(),
            },
            DisplaySnapshot {
                id: "2:0:0".into(),
                name: "内置屏".into(),
                connection: "DISPLAY2".into(),
                brightness: None,
                message: String::new(),
            },
        ];
        assert_eq!(
            json_array(&displays),
            "[\n  {\n    \"id\": \"1:0:0\",\n    \"name\": \"桌面\\t\\\"显示器\\\"\\\\\",\n    \"connection\": \"DISPLAY1\",\n    \"brightness\": 75,\n    \"message\": \"DDC/CI\\n就绪\"\n  },\n  {\n    \"id\": \"2:0:0\",\n    \"name\": \"内置屏\",\n    \"connection\": \"DISPLAY2\",\n    \"brightness\": null,\n    \"message\": \"\"\n  }\n]"
        );
        assert_eq!(json_array(&[]), "[]");
    }
}
