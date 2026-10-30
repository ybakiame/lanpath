//! MITM commands: traffic recording and CA certificate.

use lanpath_mitm::{CaOwnedTrustView, CaRemovalReceipt};
use serde::Serialize;
use tauri::State;

use crate::desktop::state::AppState;

/// External view of a traffic record.
#[derive(Debug, Clone, Serialize)]
pub struct TrafficRecordView {
    pub id: String,
    pub method: String,
    pub url: String,
    pub request_headers: Vec<(String, String)>,
    pub request_body: Option<String>,
    pub response_status: u16,
    pub response_headers: Vec<(String, String)>,
    pub response_body: Option<String>,
    pub timestamp: String,
    pub duration_ms: u64,
}

impl TrafficRecordView {
    pub(crate) fn from_record(rec: lanpath_mitm::TrafficRecord) -> Self {
        Self {
            id: rec.id.to_string(),
            method: rec.method,
            url: rec.url,
            request_headers: rec.request_headers,
            request_body: rec.request_body,
            response_status: rec.response_status,
            response_headers: rec.response_headers,
            response_body: rec.response_body,
            timestamp: rec.timestamp.to_rfc3339(),
            duration_ms: rec.duration_ms,
        }
    }
}

/// List MITM traffic records.
#[tauri::command]
pub async fn list_traffic(state: State<'_, AppState>) -> Result<Vec<TrafficRecordView>, String> {
    let lock = state.client.lock().await;
    let Some(client) = lock.as_ref() else {
        return Ok(Vec::new());
    };
    Ok(client
        .mitm_service()
        .traffic()
        .into_iter()
        .map(TrafficRecordView::from_record)
        .collect())
}

/// 获取 MITM 白名单视图：派生部分读取远程缓存（与实际生效范围一致），
/// 手动部分来自客户端配置。客户端未初始化时返回空视图。
#[tauri::command]
pub async fn get_mitm_whitelist(
    state: State<'_, AppState>,
) -> Result<lanpath_client::mitm::MitmWhitelistView, String> {
    let lock = state.client.lock().await;
    let Some(client) = lock.as_ref() else {
        return Ok(lanpath_client::mitm::MitmWhitelistView::default());
    };
    Ok(lanpath_client::mitm::MitmWhitelistView::load(&client.config.data_dir, &client.config))
}

/// Get MITM CA certificate (`data_dir/certs/ca.{crt,key}`).
#[tauri::command]
pub async fn get_mitm_ca(
    state: State<'_, AppState>,
) -> Result<lanpath_client::mitm::MitmCaView, String> {
    let service = mitm_service(&state).await;
    tauri::async_runtime::spawn_blocking(move || get_mitm_ca_impl(&service))
        .await
        .map_err(|e| format!("读取 CA 任务失败: {e}"))?
}

fn get_mitm_ca_impl(
    service: &lanpath_client::mitm::MitmService,
) -> Result<lanpath_client::mitm::MitmCaView, String> {
    service.ca()
        .map_err(|e| format!("读取 MITM CA 失败: {e}"))
}

/// Install MITM CA into the system trust store.
///
/// Windows 写 CurrentUser 存储（免 UAC）；macOS / Linux 弹系统授权框。
/// 返回安装后的信任状态（供前端直接回写缓存）。
#[tauri::command]
pub async fn install_mitm_ca(state: State<'_, AppState>, expected_sha256: String) -> Result<CaOwnedTrustView, String> {
    let service = mitm_service(&state).await;
    tauri::async_runtime::spawn_blocking(move || {
        service.install_ca(&expected_sha256)
            .map_err(|e| format!("安装 CA 到系统信任库失败: {e}"))
    })
    .await
    .map_err(|e| format!("安装 CA 任务失败: {e}"))?
}

/// Detect whether the MITM CA is trusted by the system trust store.
///
/// 原生检测失败保持 unknown；本地身份或回执损坏明确报错，不当作未安装。
#[tauri::command]
pub async fn mitm_ca_trust_status(state: State<'_, AppState>) -> Result<CaOwnedTrustView, String> {
    let service = mitm_service(&state).await;
    tauri::async_runtime::spawn_blocking(move || {
        service.trust_status()
            .map_err(|e| format!("查询 CA 信任状态失败: {e}"))
    })
    .await
    .map_err(|e| format!("查询 CA 信任状态任务失败: {e}"))?
}

#[tauri::command]
pub async fn remove_mitm_ca(state: State<'_, AppState>, expected_sha256: String) -> Result<CaRemovalReceipt, String> {
    let service = mitm_service(&state).await;
    tauri::async_runtime::spawn_blocking(move || {
        service.remove_ca(&expected_sha256)
            .map_err(|e| format!("移除拥有的 CA 信任失败: {e}"))
    }).await.map_err(|e| format!("移除 CA 信任任务失败: {e}"))?
}

async fn mitm_service(state: &AppState) -> lanpath_client::mitm::MitmService {
    if let Some(client) = state.client.lock().await.as_ref() {
        return client.mitm_service().clone();
    }
    lanpath_client::mitm::MitmService::new(
        &state.data_dir,
        std::sync::Arc::new(lanpath_client::http_exec::ReqwestHttpExecutor::for_data_dir(&state.data_dir)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDir(std::path::PathBuf);

    impl TestDir {
        fn new() -> Self {
            static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "lanpath-client-ui-test-{}-{}",
                std::process::id(),
                n
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn get_mitm_ca_generates_and_reports_path() {
        let dir = TestDir::new();
        let service = lanpath_client::mitm::MitmService::new(
            dir.path(),
            std::sync::Arc::new(lanpath_client::http_exec::ReqwestHttpExecutor::for_data_dir(dir.path())),
        );
        let view = get_mitm_ca_impl(&service).unwrap();
        assert!(
            view.pem.contains("BEGIN CERTIFICATE"),
            "pem should contain cert block: {}",
            view.pem
        );
        assert!(
            view.path.ends_with("ca.crt"),
            "path should end with ca.crt: {}",
            view.path
        );
        assert!(
            std::path::Path::new(&view.path).is_file(),
            "CA cert should be on disk: {}",
            view.path
        );

        let again = get_mitm_ca_impl(&service).unwrap();
        assert_eq!(view.pem, again.pem, "idempotent: should not regenerate");
    }
}
