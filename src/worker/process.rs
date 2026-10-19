use super::protocol::{self, Reply, Request, wait_for_reply};
use crate::display::DisplaySnapshot;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

#[derive(Default)]
pub struct ChildSlot {
    pub closed: bool,
    child: Option<Child>,
}

impl ChildSlot {
    pub fn terminate(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub type SharedChild = Arc<Mutex<ChildSlot>>;

pub struct RequestError {
    pub message: String,
    pub fatal: bool,
}

pub struct Client {
    child: SharedChild,
    input: Option<ChildStdin>,
    replies: Option<mpsc::Receiver<Result<Reply, String>>>,
    epoch: u64,
}

impl Client {
    pub fn new(child: SharedChild) -> Self {
        Self {
            child,
            input: None,
            replies: None,
            epoch: 0,
        }
    }

    fn invalidate(&mut self) {
        self.input = None;
        self.replies = None;
        self.child
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .terminate();
    }

    fn start(&mut self) -> Result<(), String> {
        self.invalidate();
        let mut slot = self.child.lock().unwrap_or_else(|error| error.into_inner());
        if slot.closed {
            return Err("应用正在退出。".into());
        }
        let mut command = Command::new(std::env::current_exe().map_err(|error| error.to_string())?);
        command
            .arg("--ddc-worker")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut child = command
            .spawn()
            .map_err(|error| format!("无法启动显示器控制进程：{error}"))?;
        let input = child.stdin.take().expect("worker stdin is piped");
        let output = child.stdout.take().expect("worker stdout is piped");
        slot.child = Some(child);
        drop(slot);
        let (sender, replies) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let reply = line.map_err(|error| error.to_string()).and_then(|line| {
                    protocol::decode_reply(&line)
                        .map_err(|error| format!("显示器控制进程响应格式错误：{error}"))
                });
                if sender.send(reply).is_err() {
                    break;
                }
            }
        });
        self.input = Some(input);
        self.replies = Some(replies);
        Ok(())
    }

    fn request(&mut self, request: Request, timeout: Duration) -> Result<Reply, RequestError> {
        let result = (|| -> Result<Reply, String> {
            let input = self
                .input
                .as_mut()
                .ok_or("控制进程不可用，请刷新显示器列表。")?;
            writeln!(input, "{}", protocol::encode_request(&request))
                .and_then(|_| input.flush())
                .map_err(|error| error.to_string())?;
            wait_for_reply(self.replies.as_ref().ok_or("控制进程未连接。")?, timeout)
        })();
        match result {
            Ok(Reply::Error(message)) => Err(RequestError {
                message,
                fatal: false,
            }),
            Ok(reply) => Ok(reply),
            Err(message) => {
                self.invalidate();
                Err(RequestError {
                    message,
                    fatal: true,
                })
            }
        }
    }

    pub fn refresh(&mut self) -> Result<Vec<DisplaySnapshot>, String> {
        self.start()?;
        self.epoch = self.epoch.wrapping_add(1);
        match self.request(
            Request::Refresh { epoch: self.epoch },
            Duration::from_secs(8),
        ) {
            Ok(Reply::Listed(displays)) => Ok(displays),
            Ok(_) => {
                self.invalidate();
                Err("显示器控制进程返回了意外响应，请刷新后重试。".into())
            }
            Err(error) => Err(error.message),
        }
    }

    pub fn set(&mut self, id: &str, percent: u8) -> Result<DisplaySnapshot, RequestError> {
        match self.request(
            Request::SetBrightness {
                id: id.into(),
                percent,
            },
            Duration::from_secs(3),
        )? {
            Reply::Updated(display) if display.id == id => Ok(display),
            _ => {
                self.invalidate();
                Err(RequestError {
                    message: "显示器控制进程返回了意外响应，请刷新后重试。".into(),
                    fatal: true,
                })
            }
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.invalidate();
    }
}
