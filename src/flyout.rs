#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct Anchor {
    pub cursor: Point,
    pub screen: Rect,
    pub work: Rect,
    pub scale: f32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TrayAction {
    Show,
    Hide,
    Ignore,
}

pub fn tray_action(visible: bool, dismissed_press: Option<u64>, released_press: u64) -> TrayAction {
    if visible {
        TrayAction::Hide
    } else if dismissed_press == Some(released_press) {
        TrayAction::Ignore
    } else {
        TrayAction::Show
    }
}

#[derive(Clone, Copy, Default)]
pub struct MousePresses {
    sequence: u64,
    down: bool,
}

impl MousePresses {
    pub fn button_down(&mut self) {
        self.sequence = self.sequence.wrapping_add(1);
        self.down = true;
    }
    pub fn button_up(&mut self) {
        self.down = false;
    }
    pub fn pressed_id(self) -> Option<u64> {
        self.down.then_some(self.sequence)
    }
    pub fn last_id(self) -> u64 {
        self.sequence
    }
}

pub fn placement(anchor: Anchor, width: i32, height: i32, margin: i32) -> Rect {
    let work = anchor.work;
    let margin = margin
        .max(0)
        .min(((work.right - work.left - 1) / 2).max(0))
        .min(((work.bottom - work.top - 1) / 2).max(0));
    let min_x = work.left + margin;
    let min_y = work.top + margin;
    let width = width.max(1).min((work.right - margin - min_x).max(1));
    let height = height.max(1).min((work.bottom - margin - min_y).max(1));
    let max_x = (work.right - margin - width).max(min_x);
    let max_y = (work.bottom - margin - height).max(min_y);
    let tail = (16.0 * anchor.scale).round() as i32;
    let mut x = (anchor.cursor.x + tail - width).clamp(min_x, max_x);
    let mut y = (anchor.cursor.y + tail - height).clamp(min_y, max_y);
    let reservations = [
        work.left - anchor.screen.left,
        work.top - anchor.screen.top,
        anchor.screen.right - work.right,
        anchor.screen.bottom - work.bottom,
    ];
    let edge = if reservations.iter().any(|gap| *gap > 0) {
        reservations
            .iter()
            .enumerate()
            .max_by_key(|(_, gap)| **gap)
            .map(|(index, _)| index)
            .unwrap_or(3)
    } else {
        [
            (anchor.cursor.x - work.left).abs(),
            (anchor.cursor.y - work.top).abs(),
            (work.right - anchor.cursor.x).abs(),
            (work.bottom - anchor.cursor.y).abs(),
        ]
        .iter()
        .enumerate()
        .min_by_key(|(_, distance)| **distance)
        .map(|(index, _)| index)
        .unwrap_or(3)
    };
    match edge {
        0 => x = min_x,
        1 => y = min_y,
        2 => x = max_x,
        _ => y = max_y,
    }
    Rect {
        left: x,
        top: y,
        right: x + width,
        bottom: y + height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bottom() -> Anchor {
        Anchor {
            cursor: Point { x: 1850, y: 1050 },
            screen: Rect {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            },
            work: Rect {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1032,
            },
            scale: 1.0,
        }
    }

    #[test]
    fn left_click_opens_a_panel_then_toggles_it_closed() {
        assert_eq!(tray_action(false, None, 1), TrayAction::Show);
        assert_eq!(tray_action(true, None, 2), TrayAction::Hide);
    }

    #[test]
    fn the_same_press_stays_suppressed_without_a_time_limit() {
        let mut mouse = MousePresses::default();
        mouse.button_down();
        let dismissed = mouse.pressed_id();
        // The press identity remains unchanged however long the button is held.
        mouse.button_up();
        assert_eq!(
            tray_action(false, dismissed, mouse.last_id()),
            TrayAction::Ignore
        );
    }

    #[test]
    fn a_different_click_after_outside_dismissal_can_open_the_panel() {
        let mut mouse = MousePresses::default();
        mouse.button_down();
        let dismissed = mouse.pressed_id();
        mouse.button_up();
        mouse.button_down();
        mouse.button_up();
        assert_eq!(
            tray_action(false, dismissed, mouse.last_id()),
            TrayAction::Show
        );
    }

    #[test]
    fn a_bottom_taskbar_opens_above_the_taskbar_inside_the_work_area() {
        let rect = placement(bottom(), 360, 240, 12);
        assert_eq!(rect.bottom, 1020);
        assert_eq!(rect.right, 1866);
        assert_eq!(rect.right - rect.left, 360);
    }

    #[test]
    fn a_negative_coordinate_monitor_keeps_its_own_panel() {
        let mut anchor = bottom();
        anchor.screen.left = -1920;
        anchor.screen.right = 0;
        anchor.work.left = -1920;
        anchor.work.right = 0;
        anchor.cursor.x = -30;
        let rect = placement(anchor, 540, 360, 18);
        assert!(rect.left >= -1902 && rect.right <= -18);
        assert_eq!(rect.bottom, 1014);
    }

    #[test]
    fn left_and_top_taskbars_do_not_get_covered() {
        let mut anchor = bottom();
        anchor.work = Rect {
            left: 48,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        anchor.cursor = Point { x: 20, y: 900 };
        assert_eq!(placement(anchor, 360, 240, 12).left, 60);
        anchor.work = Rect {
            left: 0,
            top: 48,
            right: 1920,
            bottom: 1080,
        };
        anchor.cursor = Point { x: 1800, y: 20 };
        assert_eq!(placement(anchor, 360, 240, 12).top, 60);
    }

    #[test]
    fn oversized_panels_are_clamped_to_the_available_work_area() {
        let mut anchor = bottom();
        anchor.work = Rect {
            left: 0,
            top: 0,
            right: 640,
            bottom: 400,
        };
        let rect = placement(anchor, 900, 1000, 12);
        assert!(rect.left >= 12 && rect.top >= 12 && rect.right <= 628 && rect.bottom <= 388);
    }
}
