use super::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::net::TcpListener;

#[test]
fn upstream_proxy_defaults_to_direct_and_roundtrips_json() {
    assert_eq!(UpstreamProxy::default(), UpstreamProxy::Direct);

    let direct = UpstreamProxy::Direct;
    let json = serde_json::to_string(&direct).unwrap();
    assert_eq!(json, "\"Direct\"");
    assert_eq!(
        serde_json::from_str::<UpstreamProxy>(&json).unwrap(),
        direct
    );

    let http = UpstreamProxy::Http {
        addr: "127.0.0.1:7890".parse().unwrap(),
    };
    let json = serde_json::to_string(&http).unwrap();
    assert_eq!(json, r#"{"Http":{"addr":"127.0.0.1:7890"}}"#);
    assert_eq!(serde_json::from_str::<UpstreamProxy>(&json).unwrap(), http);

    let socks5 = UpstreamProxy::Socks5 {
        addr: "[::1]:7891".parse().unwrap(),
    };
    let json = serde_json::to_string(&socks5).unwrap();
    assert_eq!(json, r#"{"Socks5":{"addr":"[::1]:7891"}}"#);
    assert_eq!(
        serde_json::from_str::<UpstreamProxy>(&json).unwrap(),
        socks5
    );
}

#[tokio::test]
async fn socks5_connect_establishes_tunnel_through_parent() {
    // 上游目标：只回显一次。
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_port = upstream.local_addr().unwrap().port();
    let upstream_task = tokio::spawn(async move {
        let (mut socket, _) = upstream.accept().await.unwrap();
        let mut buf = [0u8; 64];
        let n = socket.read(&mut buf).await.unwrap();
        let _ = socket.write_all(&buf[..n]).await;
    });

    // 极简 SOCKS5 父代理：无认证 + 域名 CONNECT + 双向转发。
    let parent = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let parent_addr = parent.local_addr().unwrap();
    let connects = Arc::new(AtomicUsize::new(0));
    let connects_for_server = Arc::clone(&connects);
    let parent_task = tokio::spawn(async move {
        loop {
            let (mut client, _) = match parent.accept().await {
                Ok(pair) => pair,
                Err(_) => break,
            };
            let connects = Arc::clone(&connects_for_server);
            tokio::spawn(async move {
                let mut greeting = [0u8; 3];
                if client.read_exact(&mut greeting).await.is_err() || greeting != [0x05, 0x01, 0x00]
                {
                    return;
                }
                let _ = client.write_all(&[0x05, 0x00]).await;
                let mut head = [0u8; 4];
                if client.read_exact(&mut head).await.is_err() || head[..3] != [0x05, 0x01, 0x00] {
                    return;
                }
                let host = match head[3] {
                    0x01 => {
                        let mut ip = [0; 4];
                        client.read_exact(&mut ip).await.unwrap();
                        std::net::Ipv4Addr::from(ip).to_string()
                    }
                    0x03 => {
                        let len = client.read_u8().await.unwrap();
                        let mut host = vec![0; len as usize];
                        client.read_exact(&mut host).await.unwrap();
                        String::from_utf8(host).unwrap()
                    }
                    _ => panic!("unexpected SOCKS address type"),
                };
                let mut port_buf = [0u8; 2];
                if client.read_exact(&mut port_buf).await.is_err() {
                    return;
                }
                let port = u16::from_be_bytes(port_buf);
                let mut target = match TcpStream::connect((host.as_str(), port)).await {
                    Ok(s) => s,
                    Err(_) => return,
                };
                connects.fetch_add(1, Ordering::SeqCst);
                let _ = client
                    .write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                    .await;
                let _ = tokio::io::copy_bidirectional(&mut client, &mut target).await;
            });
        }
    });

    let stream = TcpStream::connect(parent_addr).await.unwrap();
    let mut tunnel = socks5_connect(stream, "127.0.0.1", target_port)
        .await
        .unwrap();
    tunnel.write_all(b"ping").await.unwrap();
    let mut echo = [0u8; 4];
    tunnel.read_exact(&mut echo).await.unwrap();
    assert_eq!(&echo, b"ping");
    assert_eq!(
        connects.load(Ordering::SeqCst),
        1,
        "parent proxy should handle exactly one CONNECT"
    );

    upstream_task.abort();
    parent_task.abort();
}

#[tokio::test]
async fn http_connect_preserves_server_first_bytes_sent_with_the_response_head() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let parent = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut head = Vec::new();
        while !head.ends_with(b"\r\n\r\n") {
            head.push(socket.read_u8().await.unwrap());
        }
        assert!(head.starts_with(b"CONNECT [::1]:8443 HTTP/1.1\r\n"));
        socket
            .write_all(b"HTTP/1.1 200 established\r\nX-Parent: yes\r\n\r\n\0\xff\x80hello")
            .await
            .unwrap();
    });
    let stream = TcpStream::connect(addr).await.unwrap();
    let mut tunnel = http_connect(stream, "::1", 8443).await.unwrap();
    let mut bytes = [0; 8];
    tunnel.read_exact(&mut bytes).await.unwrap();
    assert_eq!(&bytes, b"\0\xff\x80hello");
    parent.await.unwrap();
}

#[test]
fn target_parsing_keeps_ipv6_and_tls_ports_distinct() {
    assert_eq!(
        parse_target(&"https://[::1]:8443/path".parse().unwrap()).unwrap(),
        ("::1".into(), 8443, true)
    );
    assert_eq!(
        parse_target(&"http://example.test/path".parse().unwrap()).unwrap(),
        ("example.test".into(), 80, false)
    );
}
