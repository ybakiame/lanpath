use std::{sync::Arc, time::Duration};

use futures::StreamExt as _;
use tokio::{sync::Mutex, task::JoinHandle};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{
        Error, Message,
        client::IntoClientRequest,
        http::{HeaderValue, header::AUTHORIZATION},
        protocol::WebSocketConfig,
    },
};

use crate::{
    disk_log_writer::DiskLogWriter,
    log_buffer::{CoreLogState, LogBuffer, level_rank},
};

pub(crate) struct LogService {
    buffer: Arc<LogBuffer>,
    stream: Mutex<Option<JoinHandle<()>>>,
    disk: Option<Arc<DiskLogWriter>>,
    disk_error: Option<String>,
    _disk_guards: Option<[tracing_appender::non_blocking::WorkerGuard; 2]>,
}

impl LogService {
    pub(crate) fn new(root: Option<&std::path::Path>) -> Self {
        let (disk, disk_error, guards) = match root.map(DiskLogWriter::new) {
            Some(Ok((disk, guards))) => (Some(Arc::new(disk)), None, Some(guards)),
            Some(Err(error)) => (None, Some(error), None),
            None => (None, None, None),
        };
        Self {
            buffer: Arc::new(LogBuffer::new()),
            stream: Mutex::new(None),
            disk,
            disk_error,
            _disk_guards: guards,
        }
    }

    pub(crate) fn buffer(&self) -> &Arc<LogBuffer> {
        &self.buffer
    }

    pub(crate) fn disk(&self) -> Option<&Arc<DiskLogWriter>> {
        self.disk.as_ref()
    }

    pub(crate) fn disk_error(&self) -> Option<String> {
        self.disk_error
            .clone()
            .or_else(|| self.disk.as_ref().and_then(|disk| disk.error()))
    }

    pub(crate) async fn start(&self, port: u16, secret: &str) -> Result<(), String> {
        let mut stream = self.stream.lock().await;
        if stream.is_some() {
            return Err("核心日志订阅已经运行".into());
        }
        let mut request = format!("ws://127.0.0.1:{port}/logs?level=trace")
            .into_client_request()
            .map_err(|error| format!("创建核心日志请求失败：{error}"))?;
        if !secret.is_empty() {
            let mut authorization = HeaderValue::from_str(&format!("Bearer {secret}"))
                .map_err(|_| "核心日志认证请求头无效".to_string())?;
            authorization.set_sensitive(true);
            request.headers_mut().insert(AUTHORIZATION, authorization);
        }
        let buffer = self.buffer.clone();
        let disk = self.disk.clone();
        *stream = Some(tokio::spawn(async move {
            loop {
                buffer.set_core(CoreLogState::Connecting);
                let config = WebSocketConfig::default()
                    .max_message_size(Some(64 * 1024))
                    .max_frame_size(Some(64 * 1024));
                let connected = tokio::time::timeout(
                    Duration::from_secs(5),
                    connect_async_with_config(request.clone(), Some(config), false),
                )
                .await;
                let error = match connected {
                    Ok(Ok((mut socket, _))) => {
                        buffer.set_core(CoreLogState::Live);
                        loop {
                            let result = match socket.next().await {
                                Some(Ok(Message::Text(text))) => {
                                    record_core(&buffer, disk.as_deref(), text.as_bytes())
                                }
                                Some(Ok(Message::Binary(bytes))) => {
                                    record_core(&buffer, disk.as_deref(), &bytes)
                                }
                                Some(Ok(Message::Close(_))) | None => {
                                    Err("核心日志流已关闭".into())
                                }
                                Some(Ok(_)) => continue,
                                Some(Err(error)) => Err(error.to_string()),
                            };
                            if let Err(error) = result {
                                break error;
                            }
                        }
                    }
                    Ok(Err(Error::Http(response)))
                        if response.status().is_client_error()
                            || response.status().as_u16() == 204 =>
                    {
                        buffer.set_core(CoreLogState::Unavailable(format!(
                            "核心日志接口返回 HTTP {}；重新启动核心后重试",
                            response.status()
                        )));
                        return;
                    }
                    Ok(Err(error)) => error.to_string(),
                    Err(_) => "连接核心日志接口超时".into(),
                };
                buffer.set_core(CoreLogState::Disconnected(error));
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }));
        Ok(())
    }

    pub(crate) async fn stop(&self) {
        let mut stream = self.stream.lock().await;
        if let Some(stream) = stream.take() {
            stream.abort();
            let _ = stream.await;
        }
        self.buffer.set_core(CoreLogState::Stopped);
    }
}

impl Drop for LogService {
    fn drop(&mut self) {
        if let Some(stream) = self.stream.get_mut().take() {
            stream.abort();
        }
    }
}

fn record_core(
    buffer: &LogBuffer,
    disk: Option<&DiskLogWriter>,
    bytes: &[u8],
) -> Result<(), String> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|error| format!("核心日志 JSON 无效：{error}"))?;
    let level = value
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or("核心日志缺少级别")?;
    let rank = level_rank(level).ok_or("核心日志级别无效")?;
    let message = value
        .get("payload")
        .and_then(|v| v.as_str())
        .ok_or("核心日志缺少消息")?;
    if let Some(disk) = disk {
        disk.core(level, message);
    }
    buffer.push(
        "核心",
        &level.to_ascii_uppercase(),
        rank,
        "sing-box",
        message,
        false,
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_frames_preserve_severity_and_reject_invalid_payloads() {
        let buffer = LogBuffer::new();
        record_core(
            &buffer,
            None,
            br#"{"type":"info","payload":"connection opened"}"#,
        )
        .unwrap();
        record_core(
            &buffer,
            None,
            br#"{"type":"error","payload":"connection refused"}"#,
        )
        .unwrap();
        assert!(record_core(&buffer, None, br#"{"type":"info","payload":null}"#).is_err());
        assert!(
            record_core(
                &buffer,
                None,
                br#"{"type":"unknown","payload":"not a known level"}"#
            )
            .is_err()
        );
        let errors = buffer.snapshot(None, 4, 1000).unwrap();
        assert_eq!(errors.total, 2);
        assert_eq!(
            errors.rows.iter().map(|row| row.id).collect::<Vec<_>>(),
            [2]
        );
    }
}
