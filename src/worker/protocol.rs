use crate::display::DisplaySnapshot;
use serde::{Deserialize, Serialize};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

#[derive(Debug, Deserialize, Serialize)]
pub enum Request {
    Refresh { epoch: u64 },
    SetBrightness { id: String, percent: u8 },
}

#[derive(Debug, Deserialize, Serialize)]
pub enum Reply {
    Listed(Vec<DisplaySnapshot>),
    Updated(DisplaySnapshot),
    Error(String),
}

pub fn wait_for_reply(
    replies: &Receiver<Result<Reply, String>>,
    timeout: Duration,
) -> Result<Reply, String> {
    match replies.recv_timeout(timeout) {
        Ok(reply) => reply,
        Err(RecvTimeoutError::Timeout) => Err("显示器响应超时，请检查连接后刷新。".into()),
        Err(RecvTimeoutError::Disconnected) => Err("显示器控制进程已退出，请刷新后重试。".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Instant;

    #[test]
    fn a_nonresponsive_worker_returns_a_bounded_timeout() {
        let (_sender, replies) = mpsc::channel();
        let started = Instant::now();
        let error = wait_for_reply(&replies, Duration::from_millis(20)).unwrap_err();
        assert!(error.contains("超时"));
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn a_crashed_worker_reports_disconnection_instead_of_success() {
        let (sender, replies) = mpsc::channel();
        drop(sender);
        let error = wait_for_reply(&replies, Duration::from_secs(1)).unwrap_err();
        assert!(error.contains("已退出"));
    }

    #[test]
    fn a_protocol_error_is_not_treated_as_a_device_update() {
        let (sender, replies) = mpsc::channel();
        sender.send(Err("控制进程响应格式错误".into())).unwrap();
        assert!(wait_for_reply(&replies, Duration::from_secs(1)).is_err());
    }

    #[test]
    fn a_valid_empty_snapshot_is_distinct_from_worker_failure() {
        let (sender, replies) = mpsc::channel();
        sender.send(Ok(Reply::Listed(vec![]))).unwrap();
        assert!(
            matches!(wait_for_reply(&replies, Duration::from_secs(1)), Ok(Reply::Listed(rows)) if rows.is_empty())
        );
    }
}
