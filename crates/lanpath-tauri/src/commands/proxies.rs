//! Proxy group / node commands: list, select, delay test.

use lanpath_client::{DelayResult, ProxyList};
use tauri::State;

use crate::state::AppState;

/// List all proxy groups and nodes from the running core via Clash API.
///
/// Returns error when core is not running or Clash API is unreachable;
/// frontend should show empty state.
#[tauri::command]
pub async fn proxies_list(state: State<'_, AppState>) -> Result<ProxyList, String> {
    let lock = state.client.lock().await;
    let client = lock
        .as_ref()
        .ok_or_else(|| "core not running".to_string())?;
    if !client.config.clash_api_enabled {
        return Err("Clash API is disabled".to_string());
    }
    lanpath_client::clash_get_proxies(
        client.config.clash_api_port,
        &client.config.clash_api_secret,
    )
    .await
    .map_err(|e| e.to_string())
}

/// Select a proxy in a group.
///
/// Persists the selection to `client.json` and calls Clash API `PUT /proxies/{group}`.
#[tauri::command]
pub async fn proxies_select(
    state: State<'_, AppState>,
    group: String,
    name: String,
) -> Result<(), String> {
    let lock = state.client.lock().await;
    let client = lock
        .as_ref()
        .ok_or_else(|| "core not running".to_string())?;
    if !client.config.clash_api_enabled {
        return Err("Clash API is disabled".to_string());
    }

    lanpath_client::clash_select_proxy(
        client.config.clash_api_port,
        &client.config.clash_api_secret,
        &group,
        &name,
    )
    .await
    .map_err(|e| e.to_string())?;

    lanpath_client::persist_group_selection(&state.data_dir, &group, &name)
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Test delay of a single proxy.
///
/// Uses default URL `https://www.gstatic.com/generate_204` and 5s timeout.
/// Returns `None` when the test fails or times out.
#[tauri::command]
pub async fn proxies_test_delay(
    state: State<'_, AppState>,
    name: String,
) -> Result<Option<u16>, String> {
    let lock = state.client.lock().await;
    let client = lock
        .as_ref()
        .ok_or_else(|| "core not running".to_string())?;
    if !client.config.clash_api_enabled {
        return Err("Clash API is disabled".to_string());
    }

    lanpath_client::clash_test_delay(
        client.config.clash_api_port,
        &client.config.clash_api_secret,
        &name,
        None,
        5000,
    )
    .await
    .map_err(|e| e.to_string())
}

/// Test delay for all members of a group concurrently (max 8 in flight).
///
/// Returns a vector of `(node_name, delay_ms)`; `delay_ms = None` means failure.
#[tauri::command]
pub async fn proxies_test_group(
    state: State<'_, AppState>,
    group: String,
) -> Result<Vec<DelayResult>, String> {
    let lock = state.client.lock().await;
    let client = lock
        .as_ref()
        .ok_or_else(|| "core not running".to_string())?;
    if !client.config.clash_api_enabled {
        return Err("Clash API is disabled".to_string());
    }
    lanpath_client::clash_test_group(
        client.config.clash_api_port,
        &client.config.clash_api_secret,
        &group,
    )
    .await
    .map_err(|error| error.to_string())
}
