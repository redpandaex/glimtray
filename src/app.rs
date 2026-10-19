use crate::flyout_window::Flyout;
use crate::{AppTray, MonitorRow, QuickPanel};
use glimtray::display::DisplaySnapshot;
use glimtray::queue::{PendingWrite, Revisions};
use glimtray::worker::{Controller, Update};
use slint::{ComponentHandle, Model, VecModel};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

struct State {
    rows: Rc<VecModel<MonitorRow>>,
    revisions: Revisions,
    next_revision: u64,
    scanning: bool,
}

fn row_from_snapshot(display: DisplaySnapshot) -> MonitorRow {
    MonitorRow {
        id: display.id.into(),
        name: display.name.into(),
        brightness: i32::from(display.brightness.unwrap_or(0)),
        can_control: display.brightness.is_some(),
        pending: false,
        message: display.message.into(),
    }
}

fn find_row(rows: &VecModel<MonitorRow>, id: &str) -> Option<(usize, MonitorRow)> {
    rows.iter().enumerate().find(|(_, row)| row.id == id)
}

fn refresh(ui: &QuickPanel, state: &RefCell<State>, controller: &Controller) {
    let mut state = state.borrow_mut();
    if state.scanning {
        return;
    }
    state.scanning = true;
    state.revisions.clear();
    for index in 0..state.rows.row_count() {
        if let Some(mut row) = state.rows.row_data(index) {
            row.pending = false;
            state.rows.set_row_data(index, row);
        }
    }
    ui.set_busy(true);
    ui.set_status("正在检测显示器…".into());
    controller.refresh();
}

fn apply_update(ui: &QuickPanel, state: &RefCell<State>, update: Update) {
    let mut state = state.borrow_mut();
    match update {
        Update::Listed(result) => {
            state.scanning = false;
            state.revisions.clear();
            ui.set_busy(false);
            match result {
                Ok(displays) => {
                    let supported = displays
                        .iter()
                        .filter(|display| display.brightness.is_some())
                        .count();
                    let status = if displays.is_empty() {
                        "未找到外接显示器，请检查连接和 DDC/CI 设置。".into()
                    } else {
                        format!(
                            "检测到 {} 台显示器，{} 台可调节亮度。",
                            displays.len(),
                            supported
                        )
                    };
                    state.rows.set_vec(
                        displays
                            .into_iter()
                            .map(row_from_snapshot)
                            .collect::<Vec<_>>(),
                    );
                    ui.set_status(status.into());
                }
                Err(message) => {
                    state.rows.set_vec(vec![]);
                    ui.set_status(message.into());
                }
            }
        }
        Update::Applied {
            id,
            revision,
            result,
            fatal,
        } => {
            if state.scanning {
                return;
            }
            if fatal {
                state.revisions.clear();
                for index in 0..state.rows.row_count() {
                    if let Some(mut row) = state.rows.row_data(index) {
                        row.can_control = false;
                        row.pending = false;
                        row.message = "控制连接已断开，请刷新显示器列表。".into();
                        state.rows.set_row_data(index, row);
                    }
                }
                if let Err(message) = result {
                    ui.set_status(message.into());
                }
                return;
            }
            if !state.revisions.resolve(&id, revision, result.is_ok()) {
                return;
            }
            if let Some((index, mut row)) = find_row(&state.rows, &id) {
                match result {
                    Ok(display) => {
                        row = row_from_snapshot(display);
                        ui.set_status("已读取显示器返回的亮度。".into());
                    }
                    Err(message) => {
                        row.pending = false;
                        row.can_control = false;
                        row.message = message.into();
                        ui.set_status("亮度状态未确认，请刷新后重试。".into());
                    }
                }
                state.rows.set_row_data(index, row);
            }
        }
    }
}

pub fn run() -> Result<(), slint::PlatformError> {
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .with_winit_window_attributes_hook(crate::flyout_window::window_attributes)
        .select()?;
    let ui = QuickPanel::new()?;
    let tray = AppTray::new()?;
    #[cfg(windows)]
    match glimtray::startup::Startup::current() {
        Ok(startup) => bind_startup(
            &tray,
            startup.enabled(),
            move |enabled| startup.set_enabled(enabled),
            report_error,
        ),
        Err(error) => bind_startup(&tray, Err(error), |_| unreachable!(), report_error),
    }
    let flyout = Flyout::new(&ui)?;
    let rows = Rc::new(VecModel::<MonitorRow>::default());
    ui.set_monitors(rows.clone().into());
    let state = Rc::new(RefCell::new(State {
        rows,
        revisions: Revisions::default(),
        next_revision: 0,
        scanning: false,
    }));
    let (updates, receiver) = mpsc::channel();
    let weak = ui.as_weak();
    let controller = Rc::new(Controller::new(move |update| {
        if updates.send(update).is_ok() {
            let _ = weak.upgrade_in_event_loop(|ui| ui.invoke_drain_updates());
        }
    }));
    {
        let state = state.clone();
        let weak = ui.as_weak();
        let layout = flyout.clone();
        ui.on_drain_updates(move || {
            if let Some(ui) = weak.upgrade() {
                let mut resize = false;
                for update in receiver.try_iter() {
                    resize |= matches!(
                        &update,
                        Update::Listed(_) | Update::Applied { result: Err(_), .. }
                    );
                    apply_update(&ui, &state, update);
                }
                if resize {
                    layout.refresh_layout();
                }
            }
        });
    }
    {
        let state = state.clone();
        let controller = controller.clone();
        let weak = ui.as_weak();
        ui.on_refresh(move || {
            if let Some(ui) = weak.upgrade() {
                refresh(&ui, &state, &controller);
            }
        });
    }
    {
        let state = state.clone();
        let controller = controller.clone();
        ui.on_set_brightness(move |id, percent| {
            let mut state = state.borrow_mut();
            if state.scanning {
                return;
            }
            let Some((index, mut row)) = find_row(&state.rows, &id) else {
                return;
            };
            let percent = percent.clamp(0, 100);
            if !row.can_control || row.brightness == percent {
                return;
            }
            state.next_revision = state.next_revision.wrapping_add(1);
            let revision = state.next_revision;
            state.revisions.record(&id, revision);
            row.brightness = percent;
            row.pending = true;
            state.rows.set_row_data(index, row);
            controller.set(PendingWrite {
                id: id.into(),
                percent: percent as u8,
                revision,
            });
        });
    }
    bind_flyout(&ui, &tray, &flyout);
    tray.on_quit(|| {
        let _ = slint::quit_event_loop();
    });

    let theme_timer = slint::Timer::default();
    #[cfg(windows)]
    {
        tray.set_dark_taskbar(glimtray::native::dark_taskbar());
        let weak = tray.as_weak();
        theme_timer.start(
            slint::TimerMode::Repeated,
            Duration::from_secs(5),
            move || {
                if let Some(tray) = weak.upgrade() {
                    tray.set_dark_taskbar(glimtray::native::dark_taskbar());
                }
            },
        );
    }
    tray.show()?;
    #[cfg(windows)]
    glimtray::native::finish_startup_feedback();
    refresh(&ui, &state, &controller);
    let result = slint::run_event_loop();
    theme_timer.stop();
    controller.shutdown();
    result
}

fn report_error(message: String) {
    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
        use windows::core::{PCWSTR, w};
        let text: Vec<u16> = message.encode_utf16().chain([0]).collect();
        // SAFETY: The title and message are terminated and live for the modal call.
        unsafe {
            MessageBoxW(
                None,
                PCWSTR(text.as_ptr()),
                w!("GlimTray"),
                MB_OK | MB_ICONERROR,
            );
        }
    }
    #[cfg(not(windows))]
    eprintln!("{message}");
}

fn bind_startup(
    tray: &AppTray,
    initial: Result<bool, String>,
    save: impl Fn(bool) -> Result<(), String> + 'static,
    report: impl Fn(String) + 'static,
) {
    let enabled = match initial {
        Ok(enabled) => {
            tray.set_startup_available(true);
            tray.set_startup_error("".into());
            enabled
        }
        Err(error) => {
            tray.set_startup_available(false);
            tray.set_startup_error(format!("无法读取自启设置：{error}").into());
            false
        }
    };
    tray.set_startup_enabled(enabled);
    let confirmed = Cell::new(enabled);
    let weak = tray.as_weak();
    tray.on_set_startup(move |enabled| {
        let Some(tray) = weak.upgrade() else { return };
        if !tray.get_startup_available() {
            tray.set_startup_enabled(confirmed.get());
            return;
        }
        match save(enabled) {
            Ok(()) => {
                confirmed.set(enabled);
                tray.set_startup_enabled(enabled);
                tray.set_startup_error("".into());
            }
            Err(error) => {
                tray.set_startup_enabled(confirmed.get());
                let message = format!("无法保存自启设置：{error}");
                tray.set_startup_error(message.clone().into());
                report(message);
            }
        }
    });
}

fn bind_flyout(panel: &QuickPanel, tray: &AppTray, flyout: &Flyout) {
    let weak = panel.as_weak();
    let handler = flyout.clone();
    tray.on_toggle_panel(move || match handler.toggle() {
        Ok(true) => {
            if let Some(ui) = weak.upgrade() {
                ui.invoke_refresh();
            }
        }
        Ok(false) => {}
        Err(error) => {
            report_error(format!("无法打开亮度面板：{error}"));
        }
    });
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use slint::winit_030::winit::platform::windows::EventLoopBuilderExtWindows;

    fn click_slider(panel: &QuickPanel, x: f32, y: f32) {
        use slint::platform::{PointerEventButton, WindowEvent};
        let position = slint::LogicalPosition::new(x, y);
        panel.window().dispatch_event(WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        });
        panel.window().dispatch_event(WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        });
        panel.window().dispatch_event(WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(8.0, 8.0),
        });
    }

    #[test]
    #[ignore = "requires an interactive Windows desktop; uses only an in-memory display model"]
    fn tray_click_opens_only_the_flyout_and_routes_controls() {
        let mut builder = slint::winit_030::winit::event_loop::EventLoop::<
            slint::winit_030::SlintEvent,
        >::with_user_event();
        builder.with_any_thread(true);
        slint::BackendSelector::new()
            .backend_name("winit".into())
            .with_winit_event_loop_builder(builder)
            .with_winit_window_attributes_hook(crate::flyout_window::window_attributes)
            .select()
            .unwrap();
        let panel = QuickPanel::new().unwrap();
        let tray = AppTray::new().unwrap();
        let flyout = Flyout::new(&panel).unwrap();
        let rows = Rc::new(VecModel::from(vec![MonitorRow {
            id: "test".into(),
            name: "测试显示器".into(),
            brightness: 50,
            can_control: true,
            pending: false,
            message: "DDC/CI".into(),
        }]));
        panel.set_monitors(rows.clone().into());
        panel.set_busy(false);
        let refreshes = Rc::new(Cell::new(0));
        let count = refreshes.clone();
        panel.on_refresh(move || count.set(count.get() + 1));
        let requested = Rc::new(Cell::new(0));
        let value = requested.clone();
        let model = rows.clone();
        panel.on_set_brightness(move |id, percent| {
            value.set(percent);
            if let Some((index, mut row)) = find_row(&model, &id) {
                row.brightness = percent;
                model.set_row_data(index, row);
            }
        });
        let startup_value = Rc::new(Cell::new(false));
        let startup_fail = Rc::new(Cell::new(false));
        let value = startup_value.clone();
        let fail = startup_fail.clone();
        let startup_reports = Rc::new(RefCell::new(Vec::<String>::new()));
        let reports = startup_reports.clone();
        bind_startup(
            &tray,
            Ok(false),
            move |enabled| {
                if fail.get() {
                    return Err("test save failure".into());
                }
                value.set(enabled);
                Ok(())
            },
            move |message| reports.borrow_mut().push(message),
        );
        bind_flyout(&panel, &tray, &flyout);
        tray.show().unwrap();

        let stage = Rc::new(Cell::new(0));
        let completed = stage.clone();
        let error = Rc::new(RefCell::new(None::<String>));
        let test_error = error.clone();
        let weak_panel = panel.as_weak();
        let weak_tray = tray.as_weak();
        let timer = slint::Timer::default();
        let focus_handler = flyout.clone();
        let before_pending = RefCell::new(None::<Vec<u8>>);
        timer.start(
            slint::TimerMode::Repeated,
            // Let the native widget's 150 ms hover/focus animation settle before snapshots.
            Duration::from_millis(220),
            move || {
                let panel = weak_panel.upgrade().unwrap();
                let tray = weak_tray.upgrade().unwrap();
                let outcome = (|| -> Result<(), String> {
                    match stage.get() {
                        0 => {
                            if panel.window().is_visible() {
                                return Err("startup unexpectedly shows a window".into());
                            }
                            tray.invoke_toggle_panel();
                            if !panel.window().is_visible() {
                                return Err("left click did not open only the flyout".into());
                            }
                            if refreshes.get() != 1 {
                                return Err(
                                    "opening the flyout did not request a read refresh".into()
                                );
                            }
                        }
                        1 => {
                            let pixels = panel
                                .window()
                                .take_snapshot()
                                .map_err(|error| error.to_string())?;
                            std::fs::create_dir_all("work").map_err(|error| error.to_string())?;
                            std::fs::write("work/flyout-smoke.rgba", pixels.as_bytes())
                                .map_err(|error| error.to_string())?;
                            std::fs::write(
                                "work/flyout-smoke-size.json",
                                format!("[{},{}]", pixels.width(), pixels.height()),
                            )
                            .map_err(|error| error.to_string())?;
                            *before_pending.borrow_mut() = Some(pixels.as_bytes().to_vec());
                            let mut row = rows.row_data(0).unwrap();
                            row.pending = true;
                            rows.set_row_data(0, row);
                        }
                        2 => {
                            let pixels = panel.window().take_snapshot()
                                .map_err(|error| error.to_string())?;
                            if before_pending.borrow().as_deref() != Some(pixels.as_bytes()) {
                                return Err("pending brightness changed the flyout layout".into());
                            }
                            click_slider(&panel, 180.0, 65.0);
                        }
                        3 => {
                            let pixels = panel.window().take_snapshot()
                                .map_err(|error| error.to_string())?;
                            *before_pending.borrow_mut() = Some(pixels.as_bytes().to_vec());
                            click_slider(&panel, 272.0, 56.0);
                            if requested.get() < 70 {
                                return Err("pointer input did not operate the actual slider".into());
                            }
                        }
                        4 => {
                            let mut row = rows.row_data(0).unwrap();
                            row.brightness = 50;
                            rows.set_row_data(0, row);
                        }
                        5 => {
                            let pixels = panel.window().take_snapshot()
                                .map_err(|error| error.to_string())?;
                            if before_pending.borrow().as_deref() != Some(pixels.as_bytes()) {
                                std::fs::write("work/slider-before.rgba", before_pending.borrow().as_deref().unwrap()).map_err(|error| error.to_string())?;
                                std::fs::write("work/slider-after.rgba", pixels.as_bytes()).map_err(|error| error.to_string())?;
                                return Err("slider stopped following the shared model after pointer input".into());
                            }
                            panel.invoke_set_brightness("test".into(), 64);
                            if requested.get() != 64 {
                                return Err(
                                    "panel slider is not routed to the shared controller callback"
                                        .into(),
                                );
                            }
                            tray.invoke_toggle_panel();
                            if panel.window().is_visible() {
                                return Err("second left click did not hide the flyout".into());
                            }
                        }
                        6 => {
                            tray.invoke_toggle_panel();
                            if !panel.window().is_visible() {
                                return Err("a subsequent click did not reopen the flyout".into());
                            }
                        }
                        7 => {
                            panel.window().dispatch_event(
                                slint::platform::WindowEvent::KeyPressed {
                                    text: slint::platform::Key::Escape.into(),
                                },
                            );
                            if panel.window().is_visible() {
                                return Err("Escape did not dismiss the flyout".into());
                            }
                            tray.invoke_toggle_panel();
                        }
                        8 => {
                            // Background test processes cannot reliably acquire Windows foreground focus.
                            // Feed the same focus transition handled by the native Winit event callback.
                            focus_handler.focus_changed(true);
                            focus_handler.focus_changed(false);
                        }
                        9 => {
                            if panel.window().is_visible() {
                                return Err("focus loss did not dismiss the flyout".into());
                            }
                            tray.invoke_set_startup(true);
                            if !tray.get_startup_enabled() {
                                return Err("startup state is not available in the tray menu".into());
                            }
                            if !startup_value.get() {
                                return Err("startup menu did not save the enabled state".into());
                            }
                            startup_fail.set(true);
                            tray.invoke_set_startup(false);
                            if !tray.get_startup_enabled() || tray.get_startup_error().is_empty() || startup_reports.borrow().len() != 1 {
                                return Err("failed startup save did not restore the menu state and report an error".into());
                            }
                            startup_fail.set(false);
                            tray.invoke_set_startup(false);
                            if startup_value.get() || tray.get_startup_enabled() || !tray.get_startup_error().is_empty() {
                                return Err("startup menu did not save the disabled state".into());
                            }
                            bind_startup(&tray, Err("test read failure".into()), |_| unreachable!(), |_| unreachable!());
                            if tray.get_startup_available() || tray.get_startup_error().is_empty() {
                                return Err("failed startup read did not disable the menu and report an error".into());
                            }
                            bind_startup(&tray, Ok(true), |_| Ok(()), |_| unreachable!());
                            if !tray.get_startup_enabled() || !tray.get_startup_available() || !tray.get_startup_error().is_empty() {
                                return Err("startup menu did not load the persisted enabled state".into());
                            }
                            let _ = slint::quit_event_loop();
                        }
                        _ => {}
                    }
                    Ok(())
                })();
                if let Err(message) = outcome {
                    *test_error.borrow_mut() = Some(message);
                    let _ = slint::quit_event_loop();
                }
                stage.set(stage.get() + 1);
            },
        );
        slint::run_event_loop().unwrap();
        timer.stop();
        let _ = panel.hide();
        let _ = tray.hide();
        assert!(
            error.borrow().is_none(),
            "UI regression: {:?}",
            error.borrow()
        );
        assert_eq!(completed.get(), 10);
    }
}
