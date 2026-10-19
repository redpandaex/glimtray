#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

slint::include_modules!();
mod app;
mod flyout_window;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    match std::env::args().nth(1).as_deref() {
        Some("--ddc-worker") => glimtray::worker::serve(),
        Some("--list") => {
            let mut client = glimtray::worker::Client::new(Default::default());
            let displays = client.refresh().map_err(std::io::Error::other)?;
            println!("{}", serde_json::to_string_pretty(&displays)?);
            Ok(())
        }
        Some(argument) => Err(format!("不支持的参数：{argument}。只读诊断请使用 --list。").into()),
        None => app::run().map_err(Into::into),
    }
}
