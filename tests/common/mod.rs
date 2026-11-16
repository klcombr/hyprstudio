use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::sync::Arc;

use hyprstudio::hyprland::ipc::HyprIpc;

pub struct MockIpc {
    pub ipc: HyprIpc,
    #[allow(dead_code)]
    dir: tempfile::TempDir,
}

/// A tiny stand-in for Hyprland's command socket. The handler receives the raw
/// command (e.g. `j/monitors`) and returns the response body.
pub fn mock_ipc(handler: impl Fn(&str) -> String + Send + Sync + 'static) -> MockIpc {
    let dir = tempfile::tempdir().expect("temp dir");
    let socket = dir.path().join(".socket.sock");
    let event_socket = dir.path().join(".socket2.sock");
    let listener = UnixListener::bind(&socket).expect("bind mock socket");
    let handler = Arc::new(handler);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let Ok(mut reader) = stream.try_clone() else { continue };
            let mut data = Vec::new();
            let _ = reader.read_to_end(&mut data);
            let command = String::from_utf8_lossy(&data).trim().to_string();
            let response = handler(&command);
            let _ = stream.write_all(response.as_bytes());
        }
    });
    MockIpc {
        ipc: HyprIpc::at(socket, event_socket, "mock", dir.path()),
        dir,
    }
}
