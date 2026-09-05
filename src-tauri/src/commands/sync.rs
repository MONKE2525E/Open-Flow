//! Tauri commands for LAN device sync - the Settings - Sync surface.

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::sync::manager::{DeviceInfoDto, DiscoveredDto, PairingStateDto, PeerDto, SyncManager};
use crate::sync::store as sync_store;

fn manager(app: &AppHandle) -> Result<SyncManager, String> {
    let manager = app.try_state::<SyncManager>().map(|m| m.inner().clone());
    manager.ok_or_else(|| "Sync is not available on this device.".to_string())
}

#[derive(Serialize)]
pub struct SyncStatusDto {
    pub this_device: DeviceInfoDto,
    pub listener_active: bool,
    pub pairing: Option<PairingStateDto>,
    pub discovered: Vec<DiscoveredDto>,
    pub peers: Vec<PeerDto>,
    pub last_error_hint: Option<String>,
}

#[tauri::command]
pub async fn sync_get_status(app: AppHandle) -> Result<SyncStatusDto, String> {
    let manager = manager(&app)?;
    let snapshot = manager.snapshot();
    // Surface a firewall-style hint when the listener never came up - the most
    // common "nothing shows up" cause.
    let last_error_hint = if snapshot.listener_failed {
        Some(
            "Verenu couldn't open its local network listener. Check Windows Firewall / macOS \
             Firewall settings and allow Verenu on private networks."
                .to_string(),
        )
    } else {
        None
    };
    Ok(SyncStatusDto {
        this_device: snapshot.this_device,
        listener_active: snapshot.listener_active,
        pairing: snapshot.pairing,
        discovered: snapshot.discovered,
        peers: snapshot.peers,
        last_error_hint,
    })
}

#[tauri::command]
pub async fn sync_set_device_name(app: AppHandle, name: String) -> Result<(), String> {
    manager(&app)?
        .set_device_name(name)
        .map_err(|e| e.to_string())
}

/// Starts pairing with a discovered device. Returns the 6-digit code to show
/// on this screen; the other device's user types it.
#[tauri::command]
pub async fn sync_start_pairing(app: AppHandle, device_uuid: String) -> Result<String, String> {
    manager(&app)?
        .start_pairing(device_uuid)
        .await
        .map_err(|e| e.to_string())
}

/// Responder side of pairing: approve (with the code typed by the user) or
/// reject the pending incoming request.
#[tauri::command]
pub async fn sync_respond_to_pairing(
    app: AppHandle,
    code: String,
    approve: bool,
) -> Result<(), String> {
    manager(&app)?
        .respond_to_pairing(code, approve)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sync_cancel_pairing(app: AppHandle) -> Result<(), String> {
    manager(&app)?
        .cancel_pairing()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sync_remove_device(app: AppHandle, device_uuid: String) -> Result<(), String> {
    manager(&app)?
        .remove_device(device_uuid)
        .await
        .map_err(|e| e.to_string())
}

/// Manual sync. `deviceUuid = null` syncs every visible paired device.
#[tauri::command]
pub async fn sync_now(app: AppHandle, device_uuid: Option<String>) -> Result<(), String> {
    manager(&app)?
        .sync_now(device_uuid)
        .await
        .map_err(|e| e.to_string())
}

/// Debug/diagnostics: change-log size and per-peer cursor positions.
#[tauri::command]
pub async fn sync_get_diagnostics(app: AppHandle) -> Result<serde_json::Value, String> {
    let db = app.state::<crate::DbHandle>().inner().clone();
    super::run_blocking("sync_get_diagnostics", move || {
        let conn = db
            .lock()
            .map_err(|_| "database lock poisoned".to_string())?;
        let log_size: i64 = conn
            .query_row("SELECT COUNT(*) FROM sync_log", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        let peers = sync_store::list_peers(&conn).map_err(|e| e.to_string())?;
        Ok(serde_json::json!({
            "log_entries": log_size,
            "peers": peers.into_iter().map(|p| serde_json::json!({
                "uuid": p.device_uuid,
                "name": p.name,
                "send_cursor": p.send_cursor,
                "needs_snapshot": p.needs_snapshot,
                "last_sync_at": p.last_sync_at,
                "last_error": p.last_error,
            })).collect::<Vec<_>>(),
        }))
    })
    .await
}
