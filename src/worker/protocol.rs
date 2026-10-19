use crate::display::DisplaySnapshot;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

const FIELD: char = '\t';
const FIELD_SEP: &str = "\t";
const RECORD_SEP: &str = "\x1e";

#[derive(Debug, PartialEq, Eq)]
pub enum Request {
    Refresh { epoch: u64 },
    SetBrightness { id: String, percent: u8 },
}

#[derive(Debug, PartialEq, Eq)]
pub enum Reply {
    Listed(Vec<DisplaySnapshot>),
    Updated(DisplaySnapshot),
    Error(String),
}

pub fn encode_request(request: &Request) -> String {
    match request {
        Request::Refresh { epoch } => format!("R{FIELD}{epoch}"),
        Request::SetBrightness { id, percent } => {
            format!("S{FIELD}{}{FIELD}{percent}", escape(id))
        }
    }
}

pub fn decode_request(line: &str) -> Result<Request, String> {
    let mut fields = line.split(FIELD);
    match fields.next() {
        Some("R") => {
            let epoch = fields.next().ok_or("刷新请求缺少 epoch")?;
            if fields.next().is_some() {
                return Err("刷新请求字段过多".into());
            }
            Ok(Request::Refresh {
                epoch: epoch.parse().map_err(|_| "刷新请求的 epoch 无效")?,
            })
        }
        Some("S") => {
            let id = fields.next().ok_or("亮度请求缺少设备 ID")?;
            let percent = fields.next().ok_or("亮度请求缺少百分比")?;
            if fields.next().is_some() {
                return Err("亮度请求字段过多".into());
            }
            Ok(Request::SetBrightness {
                id: unescape(id)?,
                percent: percent.parse().map_err(|_| "亮度百分比无效")?,
            })
        }
        Some(other) => Err(format!("未知请求类型：{other}")),
        None => Err("空请求".into()),
    }
}

pub fn encode_reply(reply: &Reply) -> String {
    match reply {
        Reply::Listed(displays) => {
            let mut line = String::from("L");
            if !displays.is_empty() {
                line.push(FIELD);
                let records: Vec<String> = displays.iter().map(encode_display).collect();
                line.push_str(&records.join(RECORD_SEP));
            }
            line
        }
        Reply::Updated(display) => format!("U{FIELD}{}", encode_display(display)),
        Reply::Error(message) => format!("E{FIELD}{}", escape(message)),
    }
}

pub fn decode_reply(line: &str) -> Result<Reply, String> {
    let (kind, rest) = match line.split_once(FIELD) {
        Some((kind, rest)) => (kind, Some(rest)),
        None => (line, None),
    };
    match kind {
        "L" => {
            let Some(rest) = rest.filter(|rest| !rest.is_empty()) else {
                return Ok(Reply::Listed(Vec::new()));
            };
            let displays = rest
                .split(RECORD_SEP)
                .map(decode_display)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Reply::Listed(displays))
        }
        "U" => Ok(Reply::Updated(decode_display(
            rest.ok_or("更新响应缺少设备记录")?,
        )?)),
        "E" => Ok(Reply::Error(unescape(rest.ok_or("错误响应缺少消息")?)?)),
        other => Err(format!("未知响应类型：{other}")),
    }
}

fn encode_display(display: &DisplaySnapshot) -> String {
    let brightness = display
        .brightness
        .map(|value| value.to_string())
        .unwrap_or_default();
    [
        escape(&display.id),
        escape(&display.name),
        escape(&display.connection),
        brightness,
        escape(&display.message),
    ]
    .join(FIELD_SEP)
}

fn decode_display(record: &str) -> Result<DisplaySnapshot, String> {
    let fields: Vec<&str> = record.split(FIELD).collect();
    if fields.len() != 5 {
        return Err(format!("设备记录字段数量错误：{}", fields.len()));
    }
    let brightness = if fields[3].is_empty() {
        None
    } else {
        Some(fields[3].parse().map_err(|_| "设备亮度无效")?)
    };
    Ok(DisplaySnapshot {
        id: unescape(fields[0])?,
        name: unescape(fields[1])?,
        connection: unescape(fields[2])?,
        brightness,
        message: unescape(fields[4])?,
    })
}

fn escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '\t' => escaped.push_str("\\t"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\x1e' => escaped.push_str("\\e"),
            other => escaped.push(other),
        }
    }
    escaped
}

fn unescape(value: &str) -> Result<String, String> {
    let mut unescaped = String::with_capacity(value.len());
    let mut characters = value.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            unescaped.push(character);
            continue;
        }
        match characters.next() {
            Some('\\') => unescaped.push('\\'),
            Some('t') => unescaped.push('\t'),
            Some('n') => unescaped.push('\n'),
            Some('r') => unescaped.push('\r'),
            Some('e') => unescaped.push('\x1e'),
            Some(other) => return Err(format!("未知转义：\\{other}")),
            None => return Err("转义不完整".into()),
        }
    }
    Ok(unescaped)
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

    fn display(id: &str, brightness: Option<u8>) -> DisplaySnapshot {
        DisplaySnapshot {
            id: id.into(),
            name: "显示器\t名称".into(),
            connection: "DISPLAY1".into(),
            brightness,
            message: "DDC/CI\n就绪".into(),
        }
    }

    #[test]
    fn requests_round_trip_with_escaped_ids() {
        for request in [
            Request::Refresh { epoch: 7 },
            Request::SetBrightness {
                id: "1:0:0\t\\\n\x1e".into(),
                percent: 42,
            },
        ] {
            assert_eq!(decode_request(&encode_request(&request)).unwrap(), request);
        }
    }

    #[test]
    fn replies_round_trip_records_and_empty_lists() {
        for reply in [
            Reply::Listed(vec![]),
            Reply::Listed(vec![display("a", Some(0)), display("b", None)]),
            Reply::Updated(display("c", Some(100))),
            Reply::Error("失败\n原因\t未知".into()),
        ] {
            assert_eq!(decode_reply(&encode_reply(&reply)).unwrap(), reply);
        }
    }

    #[test]
    fn malformed_requests_are_rejected() {
        for line in ["", "X", "R", "R\t1\t2", "R\tabc", "S\tid", "S\tid\t300"] {
            assert!(decode_request(line).is_err(), "{line:?}");
        }
    }

    #[test]
    fn malformed_replies_are_rejected() {
        for line in ["X", "U", "U\ttoo\tfew", "E\tbad\\zescape", "L\tid\tname"] {
            assert!(decode_reply(line).is_err(), "{line:?}");
        }
    }

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
