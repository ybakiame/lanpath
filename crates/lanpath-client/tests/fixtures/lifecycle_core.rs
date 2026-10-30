//! Test-only native child: real listeners and process lifetime, no proxy/TUN effects.
use std::{net::TcpListener, path::PathBuf, time::Duration};

fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    let config_path = args.windows(2).find(|args| args[0] == "-c")
        .map(|args| PathBuf::from(&args[1])).expect("fixture requires -c");
    let bytes = std::fs::read(config_path).unwrap();
    let config: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let capture = std::env::current_exe().unwrap().with_extension("capture");
    if capture.exists() {
        let destination = std::fs::read_to_string(capture).unwrap();
        std::fs::write(destination, &bytes).unwrap();
    }
    if let Some(inbounds) = config.get("inbounds").and_then(serde_json::Value::as_array) {
        for inbound in inbounds {
            if inbound.get("type").and_then(serde_json::Value::as_str) != Some("mixed") { continue; }
            let host = inbound.get("listen").and_then(serde_json::Value::as_str).unwrap_or("127.0.0.1");
            let port = inbound.get("listen_port").and_then(serde_json::Value::as_u64).unwrap() as u16;
            let listener = TcpListener::bind((host, port)).unwrap();
            std::thread::spawn(move || {
                for connection in listener.incoming() { drop(connection.unwrap()); }
            });
        }
    }
    loop { std::thread::sleep(Duration::from_secs(60)); }
}
