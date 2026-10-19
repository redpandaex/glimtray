use crate::QuickPanel;
use glimtray::flyout::{TrayAction, placement, tray_action};
use slint::winit_030::{EventResult, WinitWindowAccessor, winit};
use slint::{ComponentHandle, Model};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

pub const PANEL_TITLE: &str = "GlimTray · 亮度面板";

#[derive(Default)]
struct Session {
    generation: u64,
    focused: bool,
    dismissed_press: Option<u64>,
}

#[derive(Clone)]
pub struct Flyout {
    panel: slint::Weak<QuickPanel>,
    session: Rc<RefCell<Session>>,
    #[cfg(windows)]
    mouse: Rc<glimtray::native_mouse::MouseTracker>,
}

impl Flyout {
    pub fn new(panel: &QuickPanel) -> Result<Self, slint::PlatformError> {
        let flyout = Self {
            panel: panel.as_weak(),
            session: Rc::new(RefCell::new(Session::default())),
            #[cfg(windows)]
            mouse: Rc::new(
                glimtray::native_mouse::MouseTracker::new().map_err(|error| {
                    slint::PlatformError::from(format!("无法初始化托盘点击处理：{error}"))
                })?,
            ),
        };
        let handler = flyout.clone();
        panel.on_dismiss(move || handler.hide());
        panel
            .window()
            .on_close_requested(|| slint::CloseRequestResponse::HideWindow);
        let handler = flyout.clone();
        panel.window().on_winit_window_event(move |_, event| {
            if let winit::event::WindowEvent::Focused(focused) = event {
                handler.focus_changed(*focused);
            }
            EventResult::Propagate
        });
        Ok(flyout)
    }

    pub(crate) fn focus_changed(&self, focused: bool) {
        let mut session = self.session.borrow_mut();
        if focused {
            session.focused = true;
        } else if session.focused {
            session.focused = false;
            #[cfg(windows)]
            {
                session.dismissed_press = self.mouse.pressed_id();
            }
            let generation = session.generation;
            let later = self.clone();
            slint::Timer::single_shot(Duration::ZERO, move || {
                let session = later.session.borrow();
                let should_hide = session.generation == generation && !session.focused;
                drop(session);
                if should_hide {
                    later.hide();
                }
            });
        }
    }

    pub fn hide(&self) {
        let mut session = self.session.borrow_mut();
        session.generation = session.generation.wrapping_add(1);
        session.focused = false;
        drop(session);
        if let Some(panel) = self.panel.upgrade() {
            let _ = panel.hide();
        }
    }

    pub fn toggle(&self) -> Result<bool, slint::PlatformError> {
        let Some(panel) = self.panel.upgrade() else {
            return Ok(false);
        };
        #[cfg(windows)]
        let released_press = self.mouse.last_id();
        #[cfg(not(windows))]
        let released_press = 0;
        let dismissed_press = self.session.borrow_mut().dismissed_press.take();
        match tray_action(panel.window().is_visible(), dismissed_press, released_press) {
            TrayAction::Hide => {
                self.hide();
                Ok(false)
            }
            TrayAction::Ignore => Ok(false),
            TrayAction::Show => {
                self.show(&panel)?;
                Ok(true)
            }
        }
    }

    fn show(&self, panel: &QuickPanel) -> Result<(), slint::PlatformError> {
        let mut session = self.session.borrow_mut();
        session.generation = session.generation.wrapping_add(1);
        session.focused = false;
        let generation = session.generation;
        drop(session);
        let model = panel.get_monitors();
        let row_height: f32 = (0..model.row_count())
            .filter_map(|index| model.row_data(index))
            .map(|row| if row.can_control { 96.0 } else { 144.0 })
            .sum();
        let height = (76.0 + row_height.max(96.0)).min(520.0);
        panel
            .window()
            .set_size(slint::LogicalSize::new(360.0, height));
        #[cfg(windows)]
        let anchor = glimtray::native::flyout_anchor();
        #[cfg(not(windows))]
        let anchor = None;
        if let Some(anchor) = anchor {
            let bounds = placement(
                anchor,
                (360.0 * anchor.scale).round() as i32,
                (height * anchor.scale).round() as i32,
                (12.0 * anchor.scale).round() as i32,
            );
            panel
                .window()
                .set_position(slint::PhysicalPosition::new(bounds.left, bounds.top));
        }
        panel.show()?;
        panel.window().with_winit_window(|window| {
            #[cfg(windows)]
            {
                use winit::platform::windows::WindowExtWindows;
                window.set_skip_taskbar(true);
            }
            window.focus_window();
        });
        let later = self.clone();
        slint::Timer::single_shot(Duration::from_millis(20), move || {
            if later.session.borrow().generation != generation {
                return;
            }
            if let (Some(panel), Some(anchor)) = (later.panel.upgrade(), anchor) {
                let size = panel.window().size();
                let bounds = placement(
                    anchor,
                    size.width as i32,
                    size.height as i32,
                    (12.0 * anchor.scale).round() as i32,
                );
                let width = (bounds.right - bounds.left) as u32;
                let height = (bounds.bottom - bounds.top) as u32;
                if width != size.width || height != size.height {
                    panel
                        .window()
                        .set_size(slint::PhysicalSize::new(width, height));
                }
                panel
                    .window()
                    .set_position(slint::PhysicalPosition::new(bounds.left, bounds.top));
            }
        });
        Ok(())
    }
}

pub fn window_attributes(
    attributes: winit::window::WindowAttributes,
) -> winit::window::WindowAttributes {
    if attributes.title == PANEL_TITLE || !attributes.decorations {
        let attributes = attributes
            .with_resizable(false)
            .with_window_level(winit::window::WindowLevel::AlwaysOnTop);
        #[cfg(windows)]
        {
            use winit::platform::windows::WindowAttributesExtWindows;
            attributes
                .with_skip_taskbar(true)
                .with_undecorated_shadow(true)
        }
        #[cfg(not(windows))]
        {
            attributes
        }
    } else {
        attributes
    }
}
