mod process;
pub mod protocol;

use process::RequestError;
pub use process::{ChildSlot, Client, SharedChild};

use crate::display::DisplaySnapshot;
use crate::queue::{PendingWrite, PendingWrites};
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

enum Command {
    Refresh(u64),
    Write(PendingWrite),
    Shutdown,
}

pub enum Update {
    Listed(Result<Vec<DisplaySnapshot>, String>),
    Applied {
        id: String,
        revision: u64,
        result: Result<DisplaySnapshot, String>,
        fatal: bool,
    },
}

trait ControlClient: Send {
    fn refresh(&mut self) -> Result<Vec<DisplaySnapshot>, String>;
    fn set(&mut self, id: &str, percent: u8) -> Result<DisplaySnapshot, RequestError>;
}

impl ControlClient for Client {
    fn refresh(&mut self) -> Result<Vec<DisplaySnapshot>, String> {
        Client::refresh(self)
    }
    fn set(&mut self, id: &str, percent: u8) -> Result<DisplaySnapshot, RequestError> {
        Client::set(self, id, percent)
    }
}

pub struct Controller {
    sender: mpsc::Sender<Command>,
    child: SharedChild,
    thread: Mutex<Option<JoinHandle<()>>>,
    generation: Arc<AtomicU64>,
}

impl Controller {
    pub fn new(on_update: impl Fn(Update) + Send + 'static) -> Self {
        Self::with_client(Client::new, on_update)
    }

    fn with_client<T: ControlClient + 'static>(
        factory: impl FnOnce(SharedChild) -> T + Send + 'static,
        on_update: impl Fn(Update) + Send + 'static,
    ) -> Self {
        let (sender, receiver) = mpsc::channel();
        let child = Arc::new(Mutex::new(ChildSlot::default()));
        let worker_child = child.clone();
        let generation = Arc::new(AtomicU64::new(0));
        let worker_generation = generation.clone();
        let thread = thread::spawn(move || {
            let mut client = factory(worker_child);
            let mut pending = PendingWrites::default();
            let mut deadline: Option<Instant> = None;
            let mut active_generation = 0;
            let mut blocked = HashSet::new();
            let mut connected = false;
            loop {
                let command = match deadline {
                    Some(at) => receiver.recv_timeout(at.saturating_duration_since(Instant::now())),
                    None => receiver
                        .recv()
                        .map_err(|_| mpsc::RecvTimeoutError::Disconnected),
                };
                match command {
                    Ok(Command::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Ok(Command::Refresh(generation)) => {
                        pending.clear();
                        deadline = None;
                        active_generation = generation;
                        let result = client.refresh();
                        connected = result.is_ok();
                        blocked.clear();
                        on_update(Update::Listed(result));
                    }
                    Ok(Command::Write(write)) => {
                        if !connected
                            || blocked.contains(&write.id)
                            || worker_generation.load(Ordering::Acquire) != active_generation
                        {
                            continue;
                        }
                        pending.push(write);
                        deadline.get_or_insert_with(|| Instant::now() + Duration::from_millis(120));
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
                if deadline.is_some_and(|at| Instant::now() >= at) {
                    deadline = None;
                    for write in pending.drain() {
                        if !connected
                            || worker_generation.load(Ordering::Acquire) != active_generation
                        {
                            break;
                        }
                        if blocked.contains(&write.id) {
                            continue;
                        }
                        let result = client.set(&write.id, write.percent);
                        if worker_generation.load(Ordering::Acquire) != active_generation {
                            break;
                        }
                        let fatal = result.as_ref().is_err_and(|error| error.fatal);
                        if result.is_err() {
                            blocked.insert(write.id.clone());
                            if fatal {
                                connected = false;
                            }
                        }
                        on_update(Update::Applied {
                            id: write.id,
                            revision: write.revision,
                            result: result.map_err(|error| error.message),
                            fatal,
                        });
                        if fatal {
                            break;
                        }
                    }
                }
            }
        });
        Self {
            sender,
            child,
            thread: Mutex::new(Some(thread)),
            generation,
        }
    }

    pub fn refresh(&self) {
        let generation = self
            .generation
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1);
        let _ = self.sender.send(Command::Refresh(generation));
    }

    pub fn set(&self, write: PendingWrite) {
        let _ = self.sender.send(Command::Write(write));
    }

    pub fn shutdown(&self) {
        {
            let mut slot = self.child.lock().unwrap_or_else(|error| error.into_inner());
            slot.closed = true;
            slot.terminate();
        }
        let _ = self.sender.send(Command::Shutdown);
        if let Some(thread) = self
            .thread
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
        {
            let _ = thread.join();
        }
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub fn serve() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::{BufRead, Write};
    #[cfg(windows)]
    crate::native::finish_startup_feedback();
    #[cfg(windows)]
    let mut backend = crate::native::NativeBackend::default();
    let input = std::io::stdin();
    let mut output = std::io::stdout().lock();
    for line in input.lock().lines() {
        let request = serde_json::from_str::<protocol::Request>(&line?)?;
        #[cfg(windows)]
        let reply = match request {
            protocol::Request::Refresh { epoch } => {
                backend.refresh(epoch).map(protocol::Reply::Listed)
            }
            protocol::Request::SetBrightness { id, percent } => {
                backend.set(&id, percent).map(protocol::Reply::Updated)
            }
        }
        .unwrap_or_else(protocol::Reply::Error);
        #[cfg(not(windows))]
        let reply = {
            let _ = request;
            protocol::Reply::Error("当前仅支持 Windows 显示器控制。".into())
        };
        serde_json::to_writer(&mut output, &reply)?;
        writeln!(output)?;
        output.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeClient {
        failure: Option<bool>,
        first: bool,
        started: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
        calls: Arc<Mutex<Vec<(String, u8)>>>,
    }

    fn display(id: &str, percent: u8) -> DisplaySnapshot {
        DisplaySnapshot {
            id: id.into(),
            name: id.into(),
            connection: "test".into(),
            brightness: Some(percent),
            message: String::new(),
        }
    }

    impl ControlClient for FakeClient {
        fn refresh(&mut self) -> Result<Vec<DisplaySnapshot>, String> {
            Ok(vec![display("a", 50), display("b", 50)])
        }

        fn set(&mut self, id: &str, percent: u8) -> Result<DisplaySnapshot, RequestError> {
            self.calls.lock().unwrap().push((id.into(), percent));
            if self.first {
                self.first = false;
                self.started.send(()).unwrap();
                self.release.recv_timeout(Duration::from_secs(1)).unwrap();
                if let Some(fatal) = self.failure {
                    return Err(RequestError {
                        message: "injected failure".into(),
                        fatal,
                    });
                }
            }
            Ok(display(id, percent))
        }
    }

    struct Fixture {
        controller: Controller,
        updates: mpsc::Receiver<Update>,
        started: mpsc::Receiver<()>,
        release: mpsc::Sender<()>,
        calls: Arc<Mutex<Vec<(String, u8)>>>,
    }

    fn fixture(failure: Option<bool>) -> Fixture {
        let (started_sender, started) = mpsc::channel();
        let (release, release_receiver) = mpsc::channel();
        let (update_sender, updates) = mpsc::channel();
        let calls = Arc::new(Mutex::new(vec![]));
        let client = FakeClient {
            failure,
            first: true,
            started: started_sender,
            release: release_receiver,
            calls: calls.clone(),
        };
        let controller = Controller::with_client(
            move |_| client,
            move |update| {
                let _ = update_sender.send(update);
            },
        );
        controller.refresh();
        assert!(matches!(
            updates.recv_timeout(Duration::from_secs(1)).unwrap(),
            Update::Listed(Ok(_))
        ));
        Fixture {
            controller,
            updates,
            started,
            release,
            calls,
        }
    }

    fn write(id: &str, percent: u8, revision: u64) -> PendingWrite {
        PendingWrite {
            id: id.into(),
            percent,
            revision,
        }
    }

    #[test]
    fn a_failed_write_blocks_a_newer_queued_target_until_refresh() {
        let fixture = fixture(Some(false));
        fixture.controller.set(write("a", 10, 1));
        fixture
            .started
            .recv_timeout(Duration::from_secs(1))
            .unwrap();
        fixture.controller.set(write("a", 80, 2));
        fixture.release.send(()).unwrap();
        assert!(matches!(
            fixture
                .updates
                .recv_timeout(Duration::from_secs(1))
                .unwrap(),
            Update::Applied { result: Err(_), .. }
        ));
        assert!(
            fixture
                .updates
                .recv_timeout(Duration::from_millis(300))
                .is_err(),
            "a queued target was dispatched after the device failed"
        );
        assert_eq!(*fixture.calls.lock().unwrap(), vec![("a".into(), 10)]);
        fixture.controller.refresh();
        assert!(matches!(
            fixture
                .updates
                .recv_timeout(Duration::from_secs(1))
                .unwrap(),
            Update::Listed(Ok(_))
        ));
        fixture.controller.set(write("a", 70, 3));
        assert!(matches!(
            fixture
                .updates
                .recv_timeout(Duration::from_secs(1))
                .unwrap(),
            Update::Applied { result: Ok(_), .. }
        ));
    }

    #[test]
    fn a_transport_failure_blocks_other_queued_devices_until_refresh() {
        let fixture = fixture(Some(true));
        fixture.controller.set(write("a", 10, 1));
        fixture
            .started
            .recv_timeout(Duration::from_secs(1))
            .unwrap();
        fixture.controller.set(write("b", 80, 2));
        fixture.release.send(()).unwrap();
        assert!(matches!(
            fixture
                .updates
                .recv_timeout(Duration::from_secs(1))
                .unwrap(),
            Update::Applied { fatal: true, .. }
        ));
        assert!(
            fixture
                .updates
                .recv_timeout(Duration::from_millis(300))
                .is_err(),
            "another device was dispatched after transport failure"
        );
        assert_eq!(*fixture.calls.lock().unwrap(), vec![("a".into(), 10)]);
    }

    #[test]
    fn refresh_during_a_write_discards_the_old_ack_and_remaining_batch() {
        let fixture = fixture(None);
        fixture.controller.set(write("a", 10, 1));
        fixture.controller.set(write("b", 80, 2));
        fixture
            .started
            .recv_timeout(Duration::from_secs(1))
            .unwrap();
        fixture.controller.refresh();
        fixture.release.send(()).unwrap();
        assert!(
            matches!(
                fixture
                    .updates
                    .recv_timeout(Duration::from_secs(1))
                    .unwrap(),
                Update::Listed(Ok(_))
            ),
            "an old acknowledgement escaped after refresh"
        );
        assert_eq!(*fixture.calls.lock().unwrap(), vec![("a".into(), 10)]);
    }
}
