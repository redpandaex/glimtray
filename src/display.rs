use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct DisplaySnapshot {
    pub id: String,
    pub name: String,
    pub connection: String,
    pub brightness: Option<u8>,
    pub message: String,
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
}
