use std::{
    net::{SocketAddr, TcpStream},
    sync::Mutex,
    time::Duration,
};

use tauri::{Manager, WebviewWindow};
use tauri_plugin_shell::{
    ShellExt,
    process::{CommandChild, CommandEvent},
};

const ASAHI_PORT: u16 = 49306;

struct Backend(Mutex<Option<CommandChild>>);

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let db_path = data_dir.join("asahi.db");
            let db = db_path
                .to_str()
                .ok_or_else(|| "asahi database path is not valid UTF-8".to_string())?;

            let (mut rx, child) = app
                .shell()
                .sidecar("luna")?
                .args([
                    "asahi-desktop",
                    "--port",
                    &ASAHI_PORT.to_string(),
                    "--db",
                    db,
                ])
                .spawn()?;

            app.manage(Backend(Mutex::new(Some(child))));
            tauri::async_runtime::spawn(async move {
                while let Some(event) = rx.recv().await {
                    if let CommandEvent::Stdout(line) | CommandEvent::Stderr(line) = event {
                        eprint!("{}", String::from_utf8_lossy(&line));
                    }
                }
            });

            if let Some(window) = app.get_webview_window("main") {
                navigate_when_ready(window);
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                if let Some(backend) = window.try_state::<Backend>() {
                    if let Ok(mut child) = backend.0.lock() {
                        if let Some(child) = child.take() {
                            let _ = child.kill();
                        }
                    }
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("failed to run Asahi Desktop");
}

fn navigate_when_ready(window: WebviewWindow) {
    std::thread::spawn(move || {
        let addr = SocketAddr::from(([127, 0, 0, 1], ASAHI_PORT));
        for _ in 0..80 {
            if TcpStream::connect_timeout(&addr, Duration::from_millis(100)).is_ok() {
                let _ = window.eval(&format!(
                    "window.location.replace('http://127.0.0.1:{ASAHI_PORT}/')"
                ));
                return;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        let _ = window.eval("document.body.dataset.status = 'failed'");
    });
}
