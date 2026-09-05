//! The sync manager: owns the device identity, advertises and discovers peers
//! over mDNS, listens for incoming connections, runs pairing flows, schedules
//! sync sessions, and reports status to the frontend.
//!
//! Networking model (deliberately low-churn):
//! - One TCP listener on a stable per-device port, advertised via mDNS/Bonjour.
//! - A continuous mDNS browse (event-driven; no polling of peers).
//! - A sync session starts when a paired peer is discovered, when the user
//!   clicks "Sync now", or shortly after local data changes (debounced).
//! - Failed attempts back off exponentially up to 10 minutes.

use anyhow::{anyhow, Result};
use mdns_sd::{IfKind, IfPredicate, ServiceDaemon, ServiceEvent, ServiceInfo};
use rusqlite::OptionalExtension;
use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use tokio::net::TcpListener;
use tokio_rustls::server::TlsStream;

use crate::commands::validate_setting;
use crate::data::store::{self, SettingsHandle};
use crate::DbHandle;

use super::engine::{self, SyncHost};
use super::identity::{self, DeviceIdentity};
use super::pairing::{self, IdentityExchange};
use super::protocol::{read_message, send_message, Hello, Message, PROTOCOL_VERSION};
use super::store::{self as sync_store, SyncPeer};
use super::transport;

const SERVICE_TYPE: &str = "_verenu._tcp.local.";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(6);
const PAIRING_TIMEOUT: Duration = Duration::from_secs(180);
const PAIRING_PROMPT_LIFETIME: Duration = Duration::from_secs(180);
const MAX_BACKOFF: Duration = Duration::from_secs(600);
const PRIVATE_PORT_START: u16 = 49_152;
const PRIVATE_PORT_COUNT: u32 = 16_384;
const VIRTUAL_INTERFACE_MARKERS: &[&str] = &[
    "tailscale",
    "wireguard",
    "zerotier",
    "hamachi",
    "openvpn",
    "anyconnect",
    "wintun",
    "hyper-v",
    "hyperv",
    "vethernet",
    "virtualbox",
    "vmware",
    "virtual",
    "docker",
    "wsl",
    "teredo",
    "isatap",
];

fn is_tailscale_address(address: Ipv4Addr) -> bool {
    let [first, second, ..] = address.octets();
    first == 100 && (64..=127).contains(&second)
}

fn is_link_local_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => address.is_link_local(),
        IpAddr::V6(address) => address.is_unicast_link_local(),
    }
}

fn is_discovery_advertised_address_allowed(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => {
            !address.is_loopback()
                && !address.is_unspecified()
                && !address.is_link_local()
                && !is_tailscale_address(address)
        }
        IpAddr::V6(_) => false,
    }
}

pub(crate) fn is_discovery_interface_allowed(name: &str, address: IpAddr, is_p2p: bool) -> bool {
    if is_p2p || address.is_loopback() || address.is_unspecified() || is_link_local_address(address)
    {
        return false;
    }

    if matches!(address, IpAddr::V4(address) if is_tailscale_address(address)) {
        return false;
    }

    let normalized_name = name.trim().to_ascii_lowercase();
    !VIRTUAL_INTERFACE_MARKERS
        .iter()
        .any(|marker| normalized_name.contains(marker))
}

/// Keep a device on the same private TCP port across app restarts. mDNS
/// caches can retain the previous advertisement for part of its TTL; a random
/// listener port turns that otherwise harmless stale record into an immediate
/// connection refusal during pairing.
pub(crate) fn listener_port_for_uuid(uuid: &str) -> u16 {
    let hash = uuid
        .as_bytes()
        .iter()
        .fold(2_166_136_261_u32, |hash, byte| {
            (hash ^ u32::from(*byte)).wrapping_mul(16_777_619)
        });
    PRIVATE_PORT_START + (hash % PRIVATE_PORT_COUNT) as u16
}

/// Pick exactly one automatic initiator for a peer pair. Manual sync remains
/// available from either device, but discovery/pairing must not start two
/// competing sessions at once.
pub(crate) fn should_auto_initiate(local_uuid: &str, peer_uuid: &str) -> bool {
    local_uuid < peer_uuid
}

#[derive(Clone)]
pub struct SyncManager {
    pub(crate) inner: Arc<Inner>,
}

pub(crate) struct Inner {
    pub db: DbHandle,
    pub app: AppHandle,
    pub data_dir: PathBuf,
    pub identity: RwLock<Option<Arc<DeviceIdentity>>>,
    pub pending: tokio::sync::Mutex<Option<PendingPairing>>,
    pub pairing_status: Mutex<Option<PairingStatus>>,
    pub pairing_in_progress: AtomicBool,
    pub pairing_generation: AtomicU64,
    pub sessions: Mutex<HashSet<String>>,
    pub discovered: Mutex<HashMap<String, DiscoveredDevice>>,
    pub status: Mutex<HashMap<String, PeerStatus>>,
    pub backoff: Mutex<HashMap<String, Backoff>>,
    pub dirty: AtomicBool,
    pub mdns: tokio::sync::Mutex<Option<ServiceDaemon>>,
    pub listener_port: AtomicU16,
    pub available: AtomicBool,
    pub listener_failed: AtomicBool,
}

pub(crate) enum PendingPairing {
    Incoming {
        peer_uuid: String,
        peer_name: String,
        spake_msg: Vec<u8>,
        stream: Box<TlsStream<tokio::net::TcpStream>>,
        created: Instant,
        generation: u64,
    },
    Outgoing {
        generation: u64,
        abort: Option<tokio::task::AbortHandle>,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct PairingStatus {
    kind: &'static str,
    phase: &'static str,
    peer_uuid: String,
    peer_name: String,
    code: Option<String>,
    error: Option<String>,
    generation: u64,
}

#[derive(Debug, Clone)]
pub struct DiscoveredDevice {
    pub uuid: String,
    pub name: String,
    pub addresses: Vec<String>,
    pub port: u16,
    pub last_seen_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerState {
    Offline,
    Connecting,
    Syncing,
    Synced,
    Error,
}

#[derive(Debug, Clone)]
pub struct PeerStatus {
    pub state: PeerState,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct Backoff {
    failures: u32,
    next_attempt: Instant,
}

// ---- DTOs surfaced to the frontend ----

#[derive(Debug, Clone, serde::Serialize)]
pub struct DeviceInfoDto {
    pub uuid: String,
    pub name: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DiscoveredDto {
    pub uuid: String,
    pub name: String,
    pub addresses: Vec<String>,
    pub port: u16,
    pub paired: bool,
    pub last_seen_ms: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PeerDto {
    pub uuid: String,
    pub name: String,
    pub added_at: Option<String>,
    pub last_sync_at: Option<String>,
    pub state: String,
    pub error: Option<String>,
    pub online: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PairingStateDto {
    pub kind: String,
    pub phase: String,
    pub peer_uuid: String,
    pub peer_name: String,
    pub code: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncStatusSnapshot {
    pub this_device: DeviceInfoDto,
    pub listener_active: bool,
    pub listener_failed: bool,
    pub pairing: Option<PairingStateDto>,
    pub discovered: Vec<DiscoveredDto>,
    pub peers: Vec<PeerDto>,
}

impl SyncManager {
    /// Initializes the identity and starts all background tasks. Sync stays
    /// soft-failed if identity/listener setup fails - the rest of the app must
    /// never refuse to start over LAN sync.
    pub fn start(app: AppHandle, db: DbHandle) -> SyncManager {
        let data_dir = crate::app_setup::app_data_dir();
        let inner = Arc::new(Inner {
            db,
            app: app.clone(),
            data_dir: data_dir.clone(),
            identity: RwLock::new(None),
            pending: tokio::sync::Mutex::new(None),
            pairing_status: Mutex::new(None),
            pairing_in_progress: AtomicBool::new(false),
            pairing_generation: AtomicU64::new(0),
            sessions: Mutex::new(HashSet::new()),
            discovered: Mutex::new(HashMap::new()),
            status: Mutex::new(HashMap::new()),
            backoff: Mutex::new(HashMap::new()),
            dirty: AtomicBool::new(false),
            mdns: tokio::sync::Mutex::new(None),
            listener_port: AtomicU16::new(0),
            available: AtomicBool::new(false),
            listener_failed: AtomicBool::new(false),
        });
        let manager = SyncManager { inner };

        let init = manager.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(err) = init.initialize().await {
                init.inner.listener_failed.store(true, Ordering::Relaxed);
                log::error!("sync: disabled - {err:#}");
            }
        });
        let _ = app;
        manager
    }

    async fn initialize(&self) -> Result<()> {
        let known_uuid = {
            let conn = self.lock_db()?;
            sync_store::self_uuid(&conn)?
        };
        let identity = identity::load_or_create(&self.inner.data_dir, known_uuid)
            .map_err(|e| anyhow!("identity setup failed ({e}); check the OS credential store"))?;
        let identity = Arc::new(identity);

        // Prefer a user-set name persisted in the DB over the hostname default.
        let name = {
            let conn = self.lock_db()?;
            let stored = stored_device_name(&conn)?;
            let name = match stored {
                Some(name) if !name.trim().is_empty() => name,
                _ => identity.name.clone(),
            };
            sync_store::ensure_self_identity(&conn, &identity.uuid, &name)?;
            name
        };
        {
            let mut guard = self.inner.identity.write().expect("identity lock");
            let mut identity = (*identity).clone();
            identity.name = name;
            *guard = Some(Arc::new(identity));
        }

        // TLS configs (client configs are built per connection; the identity
        // can be rotated by re-initialization).
        let (uuid, cert, key) = {
            let guard = self.inner.identity.read().expect("identity lock");
            let identity = guard.as_ref().ok_or_else(|| anyhow!("identity missing"))?;
            (
                identity.uuid.clone(),
                identity.cert_der().clone(),
                identity.tls_key(),
            )
        };
        let server_cfg = transport::server_config(cert, key)?;

        // Reuse a deterministic port so an mDNS record cached across an app
        // restart still points at the live listener. Fall back only for a real
        // local collision, such as two Verenu identities running together.
        let preferred_port = listener_port_for_uuid(&uuid);
        let listener = match TcpListener::bind(("0.0.0.0", preferred_port)).await {
            Ok(listener) => listener,
            Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => {
                log::warn!(
                    "sync: preferred listener port {preferred_port} is occupied; using an ephemeral port"
                );
                TcpListener::bind("0.0.0.0:0")
                    .await
                    .map_err(|fallback| anyhow!("could not bind sync listener: {fallback}"))?
            }
            Err(err) => return Err(anyhow!("could not bind sync listener: {err}")),
        };
        let port = listener
            .local_addr()
            .map_err(|e| anyhow!("sync listener has no address: {e}"))?
            .port();
        self.inner.listener_port.store(port, Ordering::Relaxed);
        self.inner.available.store(true, Ordering::Relaxed);
        log::info!("sync: listening on port {port}");

        let accept_inner = self.inner.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((tcp, _)) => {
                        let inner = accept_inner.clone();
                        let cfg = server_cfg.clone();
                        tauri::async_runtime::spawn(async move {
                            let acceptor = tokio_rustls::TlsAcceptor::from(cfg);
                            match acceptor.accept(tcp).await {
                                Ok(tls) => {
                                    handle_connection(inner, tls).await;
                                }
                                Err(err) => {
                                    log::debug!("sync: tls accept failed: {err}");
                                }
                            }
                        });
                    }
                    Err(err) => {
                        log::error!("sync: listener accept failed: {err}");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        });

        // One-time repair for exe targets written before cross-platform
        // resolution existed (or before it was applied to that row): a target
        // that doesn't look like this platform's own naming convention and
        // was never tagged gets a chance to resolve against apps installed
        // right now. Best-effort ? sync must start regardless.
        if let Err(err) = self.reconcile_stale_context_targets() {
            log::warn!("sync: stale context target reconciliation failed: {err:#}");
        }

        // Discovery.
        self.start_discovery().await?;

        // Change-driven + fallback sync scheduling.
        let monitor = self.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;
                monitor.monitor_tick().await;
            }
        });

        let _ = self.inner.app.emit("verenu:sync-devices-changed", ());
        Ok(())
    }

    async fn start_discovery(&self) -> Result<()> {
        if let Some(previous) = self.inner.mdns.lock().await.take() {
            let _ = previous.shutdown();
        }
        let daemon =
            ServiceDaemon::new().map_err(|e| anyhow!("mDNS daemon failed to start: {e}"))?;
        daemon
            .disable_interface(IfKind::Predicate(IfPredicate::new(|interface| {
                !is_discovery_interface_allowed(&interface.name, interface.ip(), interface.is_p2p())
            })))
            .map_err(|e| anyhow!("mDNS interface filtering failed: {e}"))?;
        let (uuid, name, port) = {
            let guard = self.inner.identity.read().expect("identity lock");
            let identity = guard.as_ref().ok_or_else(|| anyhow!("identity missing"))?;
            (
                identity.uuid.clone(),
                identity.name.clone(),
                self.inner.listener_port.load(Ordering::Relaxed),
            )
        };
        let instance = format!("verenu-{uuid}");
        // Namespace the DNS host separately from the service instance. Older
        // builds published the UUID host as 127.0.0.1 on macOS; using a fresh,
        // app-specific host prevents that stale A record from hiding the LAN IP.
        let host = format!("verenu-{uuid}.local.");
        let props: HashMap<String, String> = [
            ("uuid", uuid.clone()),
            ("name", name),
            ("ver", PROTOCOL_VERSION.to_string()),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
        // mdns-sd's automatic address selection can classify active macOS
        // interfaces as Unknown and publish only 127.0.0.1. Enumerate usable
        // IPv4 interfaces ourselves so peers receive a reachable LAN address.
        let mut lan_addresses: Vec<std::net::IpAddr> = if_addrs::get_if_addrs()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|interface| {
                let address = interface.ip();
                (is_discovery_interface_allowed(&interface.name, address, interface.is_p2p())
                    && is_discovery_advertised_address_allowed(address))
                .then_some(address)
            })
            .collect();
        // Some directly launched macOS app binaries see only loopback through
        // getifaddrs. A UDP route lookup does not send traffic and reliably
        // reveals the primary interface address in that environment.
        if let Ok(route_probe) = std::net::UdpSocket::bind("0.0.0.0:0") {
            if route_probe.connect("192.0.2.1:9").is_ok() {
                if let Ok(local) = route_probe.local_addr() {
                    let address = local.ip();
                    if is_discovery_advertised_address_allowed(address) {
                        lan_addresses.push(address);
                    }
                }
            }
        }
        lan_addresses.sort_unstable();
        lan_addresses.dedup();
        if lan_addresses.is_empty() {
            log::warn!(
                "sync: no usable LAN address found; keeping discovery alive without advertising"
            );
        } else {
            log::info!("sync: advertising {host} on {lan_addresses:?}");
            let service = ServiceInfo::new(
                SERVICE_TYPE,
                &instance,
                &host,
                lan_addresses.as_slice(),
                port,
                Some(props),
            )
            .map_err(|e| anyhow!("mDNS service info invalid: {e}"))?;
            daemon
                .register(service)
                .map_err(|e| anyhow!("mDNS registration failed: {e}"))?;
        }

        let receiver = daemon
            .browse(SERVICE_TYPE)
            .map_err(|e| anyhow!("mDNS browse failed: {e}"))?;
        *self.inner.mdns.lock().await = Some(daemon);

        let inner = self.inner.clone();
        tauri::async_runtime::spawn(async move {
            while let Ok(event) = receiver.recv_async().await {
                match event {
                    ServiceEvent::ServiceResolved(info) => {
                        handle_resolved(&inner, *info);
                    }
                    ServiceEvent::ServiceRemoved(_, fullname) => {
                        let instance_name = fullname.split('.').next().unwrap_or("");
                        let instance = instance_name
                            .strip_prefix("verenu-")
                            .unwrap_or(instance_name);
                        let changed = inner
                            .discovered
                            .lock()
                            .map(|mut d| d.remove(instance).is_some())
                            .unwrap_or(false);
                        if changed {
                            inner.emit_devices_changed();
                        }
                    }
                    _ => {}
                }
            }
        });
        Ok(())
    }

    fn lock_db(&self) -> Result<std::sync::MutexGuard<'_, rusqlite::Connection>> {
        self.inner
            .db
            .lock()
            .map_err(|_| anyhow!("database lock was poisoned"))
    }

    /// Repairs `context_targets` rows that predate cross-platform resolution
    /// (or were synced in before this device ever ran it): an untagged row
    /// whose executable doesn't look like this platform's own naming
    /// convention gets resolved against apps installed right now, mirroring
    /// what `resolve_context_targets_in_ops` already does for every live
    /// incoming sync op. A match is tagged with this platform so it now
    /// displays only here; no match gets the same "?::" unresolved marker the
    /// live path uses, so the UI shows a clear "(not found)" chip instead of
    /// a bare foreign string a user can't act on. Best-effort: errors are
    /// logged by the caller, never fatal to sync startup.
    fn reconcile_stale_context_targets(&self) -> Result<()> {
        let conn = self.lock_db()?;
        let rows: Vec<(i64, String)> = {
            let mut stmt =
                conn.prepare("SELECT id, executable FROM context_targets WHERE platform IS NULL")?;
            let mapped = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?;
            mapped
        };
        if rows.is_empty() {
            return Ok(());
        }

        let native_suffix = if cfg!(target_os = "macos") {
            ".app"
        } else {
            ".exe"
        };
        let installed_apps = crate::system::apps::list_installed_apps();
        let platform_tag = crate::data::db::current_platform_tag();

        for (id, executable) in rows {
            if executable.starts_with("?::") || executable.to_lowercase().ends_with(native_suffix) {
                continue;
            }
            let mut resolved = closest_installed_app(&executable, &installed_apps)
                .map(|app| app.exe.clone())
                .unwrap_or_else(|| engine::unresolved_app_target(&executable));
            if resolved.starts_with("?::") {
                let existing_id: Option<i64> = conn
                    .query_row(
                        "SELECT id FROM context_targets WHERE executable = ?1",
                        rusqlite::params![resolved],
                        |row| row.get(0),
                    )
                    .optional()?;
                if existing_id.is_some_and(|existing_id| existing_id != id) {
                    resolved = format!("{resolved}#{id}");
                }
            }
            let new_platform = (!resolved.starts_with("?::"))
                .then_some(platform_tag)
                .flatten();
            let updated = conn.execute(
                "UPDATE context_targets SET executable = ?1, platform = ?2 WHERE id = ?3",
                rusqlite::params![resolved, new_platform, id],
            );
            match updated {
                Ok(_) => {}
                // The resolved executable already belongs to another row for
                // this context (both devices' targets turned out to be the
                // same app) ? drop the now-redundant stale row instead of
                // erroring the whole pass.
                Err(rusqlite::Error::SqliteFailure(err, _))
                    if err.code == rusqlite::ErrorCode::ConstraintViolation =>
                {
                    conn.execute(
                        "DELETE FROM context_targets WHERE id = ?1",
                        rusqlite::params![id],
                    )?;
                }
                Err(err) => return Err(err.into()),
            }
        }
        Ok(())
    }

    pub fn identity_exchange(&self) -> Result<IdentityExchange> {
        let guard = self.inner.identity.read().expect("identity lock");
        let identity = guard
            .as_ref()
            .ok_or_else(|| anyhow!("sync is unavailable"))?;
        Ok(IdentityExchange {
            device_uuid: identity.uuid.clone(),
            device_name: identity.name.clone(),
            cert_der: identity.cert_der().as_ref().to_vec(),
        })
    }

    pub fn device_info(&self) -> DeviceInfoDto {
        let guard = self.inner.identity.read().expect("identity lock");
        match guard.as_ref() {
            Some(identity) => DeviceInfoDto {
                uuid: identity.uuid.clone(),
                name: identity.name.clone(),
            },
            None => DeviceInfoDto {
                uuid: String::new(),
                name: String::new(),
            },
        }
    }

    pub fn snapshot(&self) -> SyncStatusSnapshot {
        let this_device = self.device_info();
        let peers = conn_peers(&self.inner.db);
        let paired: HashSet<String> = paired_set(peers.clone());
        let discovered = self
            .inner
            .discovered
            .lock()
            .map(|map| {
                let mut list: Vec<DiscoveredDto> = map
                    .values()
                    .map(|d| DiscoveredDto {
                        uuid: d.uuid.clone(),
                        name: d.name.clone(),
                        addresses: d.addresses.clone(),
                        port: d.port,
                        paired: paired.contains(&d.uuid),
                        last_seen_ms: d.last_seen_ms,
                    })
                    .collect();
                list.sort_by_key(|a| a.name.to_lowercase());
                list
            })
            .unwrap_or_default();
        let status = self.inner.status.lock().ok();
        let peers = peers
            .into_iter()
            .map(|peer| {
                let online = discovered.iter().any(|d| d.uuid == peer.device_uuid);
                let state = status
                    .as_ref()
                    .and_then(|s| s.get(&peer.device_uuid))
                    .cloned()
                    .unwrap_or(PeerStatus {
                        state: PeerState::Offline,
                        error: peer.last_error.clone(),
                    });
                PeerDto {
                    uuid: peer.device_uuid.clone(),
                    name: if peer.name.is_empty() {
                        discovered
                            .iter()
                            .find(|d| d.uuid == peer.device_uuid)
                            .map(|d| d.name.clone())
                            .unwrap_or_else(|| "Paired device".to_string())
                    } else {
                        peer.name
                    },
                    added_at: Some(peer.added_at),
                    last_sync_at: peer.last_sync_at,
                    state: state_string(state.state),
                    error: state.error.or(peer.last_error),
                    online,
                }
            })
            .collect();
        let pairing = self
            .inner
            .pairing_status
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().cloned())
            .map(|pairing| PairingStateDto {
                kind: pairing.kind.to_string(),
                phase: pairing.phase.to_string(),
                peer_uuid: pairing.peer_uuid,
                peer_name: pairing.peer_name,
                code: pairing.code,
                error: pairing.error,
            });
        SyncStatusSnapshot {
            this_device,
            listener_active: self.inner.available.load(Ordering::Relaxed),
            listener_failed: self.inner.listener_failed.load(Ordering::Relaxed),
            pairing,
            discovered,
            peers,
        }
    }

    /// Initiates pairing with a discovered device. Returns the code to show.
    pub async fn start_pairing(&self, peer_uuid: String) -> Result<String> {
        let target = self
            .inner
            .discovered
            .lock()
            .map_err(|_| anyhow!("discovery lock poisoned"))?
            .get(&peer_uuid)
            .cloned()
            .ok_or_else(|| anyhow!("that device is no longer visible on the network"))?;
        let generation;
        {
            let mut pending = self.inner.pending.lock().await;
            if pending.is_some() {
                return Err(anyhow!("a pairing is already in progress"));
            }
            generation = self
                .inner
                .pairing_generation
                .fetch_add(1, Ordering::Relaxed)
                + 1;
            *pending = Some(PendingPairing::Outgoing {
                generation,
                abort: None,
            });
        }
        let code = pairing::generate_pairing_code();
        self.inner.set_pairing_status(PairingStatus {
            kind: "outgoing",
            phase: "connecting",
            peer_uuid: peer_uuid.clone(),
            peer_name: target.name.clone(),
            code: Some(code.clone()),
            error: None,
            generation,
        });

        let manager = self.clone();
        let task_code = code.clone();
        let task = tokio::spawn(async move {
            let result = manager
                .run_outgoing_pairing(&target, &task_code, generation)
                .await;
            if let Err(err) = result {
                log::warn!("sync: pairing with {} failed: {err:#}", target.name);
                manager.inner.fail_pairing(generation, format!("{err:#}"));
                manager
                    .inner
                    .app
                    .emit(
                        "verenu:sync-pair-result",
                        serde_json::json!({
                            "uuid": target.uuid,
                            "ok": false,
                            "message": format!("{err:#}"),
                        }),
                    )
                    .ok();
            }
            manager.clear_pending_if_generation(generation).await;
            manager.inner.emit_devices_changed();
        });
        let abort = task.abort_handle();
        {
            let mut pending = self.inner.pending.lock().await;
            if let Some(PendingPairing::Outgoing {
                abort: slot,
                generation: g,
                ..
            }) = pending.as_mut()
            {
                if *g == generation {
                    *slot = Some(abort);
                }
            }
        }
        Ok(code)
    }

    async fn run_outgoing_pairing(
        &self,
        target: &DiscoveredDevice,
        code: &str,
        generation: u64,
    ) -> Result<()> {
        let identity = self.identity_exchange()?;
        if target.addresses.is_empty() {
            return Err(anyhow!("device has no reachable address"));
        }
        let client_cfg = {
            let guard = self.inner.identity.read().expect("identity lock");
            let identity = guard.as_ref().ok_or_else(|| anyhow!("sync unavailable"))?;
            transport::client_config(identity.cert_der().clone(), identity.tls_key())?
        };
        let connector = transport::tls_connector(client_cfg);
        let mut tcp = None;
        let mut failures = Vec::new();
        for addr in connection_candidates(&target.addresses, target.port, &target.uuid) {
            match tokio::time::timeout(CONNECT_TIMEOUT, tokio::net::TcpStream::connect(addr)).await
            {
                Ok(Ok(stream)) => {
                    tcp = Some(stream);
                    break;
                }
                Ok(Err(err)) => failures.push(format!("{addr}: {err}")),
                Err(_) => failures.push(format!("{addr}: timed out")),
            }
        }
        let tcp = tcp.ok_or_else(|| {
            anyhow!(
                "could not reach {} on any discovered address: {}",
                target.name,
                failures.join("; ")
            )
        })?;
        let mut tls = tokio::time::timeout(
            CONNECT_TIMEOUT,
            connector.connect(transport::server_name_for(&target.uuid), tcp),
        )
        .await
        .map_err(|_| anyhow!("timed out during TLS handshake"))?
        .map_err(|e| anyhow!("TLS handshake with {} failed: {e}", target.name))?;

        let (spake_state, spake_msg) = pairing::initiator_start(code);
        send_message(
            &mut tls,
            &Message::PairRequest {
                device_uuid: identity.device_uuid.clone(),
                device_name: identity.device_name.clone(),
                protocol: PROTOCOL_VERSION,
                spake_msg,
            },
        )
        .await?;
        self.inner
            .update_pairing_phase(generation, "waiting_for_code");

        // The responder replies only after its user approves, so the whole
        // exchange runs under the generous pairing timeout.
        let exchange = async {
            let responder_msg = match read_message(&mut tls).await? {
                Message::PairAccept { spake_msg } => spake_msg,
                Message::PairReject { reason } => return Err(anyhow!("rejected: {reason}")),
                Message::PairBusy => {
                    return Err(anyhow!(
                        "{} is already handling another pairing",
                        target.name
                    ))
                }
                Message::Error { message } => return Err(anyhow!("{message}")),
                other => return Err(anyhow!("unexpected pairing response: {other:?}")),
            };
            let cipher = pairing::initiator_cipher(spake_state, &responder_msg)?;
            pairing::initiator_exchange(&mut tls, &cipher, &identity, &target.uuid).await
        };
        let outcome = tokio::time::timeout(PAIRING_TIMEOUT, exchange)
            .await
            .map_err(|_| anyhow!("{} didn't complete the pairing in time", target.name))??;
        self.complete_pairing(outcome, generation).await
    }

    /// Responder side: user approved (or rejected) with the typed code.
    pub async fn respond_to_pairing(&self, code: String, approve: bool) -> Result<()> {
        let pending = {
            let mut guard = self.inner.pending.lock().await;
            let pending = match guard.take() {
                Some(PendingPairing::Incoming {
                    peer_uuid,
                    peer_name,
                    spake_msg,
                    stream,
                    created,
                    generation,
                }) => {
                    let mut stream = stream;
                    if !approve {
                        let _ = send_message(
                            &mut stream,
                            &Message::PairReject {
                                reason: "declined".to_string(),
                            },
                        )
                        .await;
                        self.inner
                            .app
                            .emit(
                                "verenu:sync-pair-result",
                                serde_json::json!({ "uuid": peer_uuid, "ok": false, "message": "Pairing declined" }),
                            )
                            .ok();
                        self.inner.clear_pairing_status(generation);
                        return Ok(());
                    }
                    (peer_uuid, peer_name, spake_msg, stream, created, generation)
                }
                Some(PendingPairing::Outgoing { .. }) | None => {
                    return Err(anyhow!("no incoming pairing request to respond to"));
                }
            };
            self.inner
                .pairing_in_progress
                .store(true, Ordering::Release);
            pending
        };
        let _pairing_guard = PairingInProgressGuard(&self.inner.pairing_in_progress);
        let (peer_uuid, peer_name, spake_msg, mut stream, created, generation) = pending;
        self.inner.update_pairing_phase(generation, "verifying");
        let identity = match self.identity_exchange() {
            Ok(identity) => identity,
            Err(err) => {
                self.restore_incoming_pairing(
                    peer_uuid.clone(),
                    peer_name.clone(),
                    spake_msg.clone(),
                    stream,
                    created,
                    generation,
                )
                .await;
                return Err(err);
            }
        };
        let (responder_msg, cipher) = match pairing::responder_start(&code, &spake_msg) {
            Ok(result) => result,
            Err(err) => {
                // A mistyped code is retryable; the SPAKE exchange has not
                // touched the stream yet, so keep the incoming request alive.
                self.restore_incoming_pairing(
                    peer_uuid.clone(),
                    peer_name.clone(),
                    spake_msg.clone(),
                    stream,
                    created,
                    generation,
                )
                .await;
                return Err(err);
            }
        };
        let result = match tokio::time::timeout(
            PAIRING_TIMEOUT,
            pairing::responder_exchange(&mut stream, &cipher, responder_msg, &identity, &peer_uuid),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(anyhow!("pairing timed out")),
        };
        match result {
            Ok(outcome) => {
                if let Err(err) = self.complete_pairing(outcome, generation).await {
                    self.inner.fail_pairing(generation, format!("{err:#}"));
                    return Err(err);
                }
                self.inner
                    .app
                    .emit(
                        "verenu:sync-pair-result",
                        serde_json::json!({ "uuid": peer_uuid, "ok": true, "message": format!("Paired with {peer_name}") }),
                    )
                    .ok();
            }
            Err(err) => {
                self.inner.fail_pairing(generation, format!("{err:#}"));
                self.inner
                    .app
                    .emit(
                        "verenu:sync-pair-result",
                        serde_json::json!({ "uuid": peer_uuid, "ok": false, "message": format!("{err:#}") }),
                    )
                    .ok();
                return Err(err);
            }
        }
        Ok(())
    }

    async fn restore_incoming_pairing(
        &self,
        peer_uuid: String,
        peer_name: String,
        spake_msg: Vec<u8>,
        stream: Box<TlsStream<tokio::net::TcpStream>>,
        created: Instant,
        generation: u64,
    ) {
        let mut pending = self.inner.pending.lock().await;
        if pending.is_none() && self.inner.pairing_generation.load(Ordering::Relaxed) == generation
        {
            *pending = Some(PendingPairing::Incoming {
                peer_uuid: peer_uuid.clone(),
                peer_name: peer_name.clone(),
                spake_msg,
                stream,
                created,
                generation,
            });
            self.inner.set_pairing_status(PairingStatus {
                kind: "incoming",
                phase: "awaiting_code",
                peer_uuid: peer_uuid.clone(),
                peer_name: peer_name.clone(),
                code: None,
                error: None,
                generation,
            });
            self.inner.emit_devices_changed();
        }
    }

    async fn complete_pairing(&self, outcome: IdentityExchange, generation: u64) -> Result<()> {
        if outcome.device_uuid.is_empty() || outcome.cert_der.is_empty() {
            return Err(anyhow!("peer sent an incomplete identity"));
        }
        // Serialize the generation check with cancellation/new pairing so a
        // cancelled exchange cannot persist a peer after the user moved on.
        let _pairing_generation_guard = self.inner.pending.lock().await;
        if self.inner.pairing_generation.load(Ordering::Relaxed) != generation {
            return Err(anyhow!("pairing was cancelled"));
        }
        let fp = identity::fingerprint_of(&outcome.cert_der);
        {
            let conn = self.lock_db()?;
            sync_store::upsert_peer(&conn, &outcome.device_uuid, &outcome.device_name, &fp)?;
            // Seed setting stamps at pairing time so the first session compares
            // real timestamps instead of treating existing values as ancient.
            let keys: Vec<String> = engine::SYNCABLE_SETTINGS
                .iter()
                .map(|s| s.to_string())
                .collect();
            sync_store::seed_setting_stamps(&conn, &self.device_info().uuid, &keys)?;
        }
        drop(_pairing_generation_guard);
        self.clear_pending_if_generation(generation).await;
        self.inner.clear_pairing_status(generation);
        self.inner
            .app
            .emit(
                "verenu:sync-pair-result",
                serde_json::json!({
                    "uuid": outcome.device_uuid,
                    "ok": true,
                    "message": format!("Paired with {}", outcome.device_name),
                }),
            )
            .ok();
        self.inner.emit_devices_changed();
        // Pull right away, from one deterministic side only. Both peers finish
        // pairing at nearly the same time and used to open competing sessions.
        let uuid = outcome.device_uuid.clone();
        if should_auto_initiate(&self.device_info().uuid, &uuid) {
            let manager = self.clone();
            tauri::async_runtime::spawn(async move {
                manager.sync_to_peer(&uuid).await;
            });
        }
        Ok(())
    }

    pub async fn cancel_pairing(&self) -> Result<()> {
        let mut guard = self.inner.pending.lock().await;
        self.inner
            .pairing_generation
            .fetch_add(1, Ordering::Relaxed);
        if let Some(PendingPairing::Outgoing {
            abort: Some(abort_handle),
            ..
        }) = guard.take()
        {
            abort_handle.abort();
        }
        if let Ok(mut pairing) = self.inner.pairing_status.lock() {
            *pairing = None;
        }
        self.inner.emit_devices_changed();
        Ok(())
    }

    async fn clear_pending_if_generation(&self, generation: u64) {
        let mut guard = self.inner.pending.lock().await;
        let stored_generation = match guard.as_ref() {
            Some(PendingPairing::Outgoing { generation: g, .. }) => Some(*g),
            Some(PendingPairing::Incoming { generation: g, .. }) => Some(*g),
            None => None,
        };
        if stored_generation == Some(generation) {
            *guard = None;
        }
    }

    /// Removes a paired device locally and best-effort notifies the peer.
    pub async fn remove_device(&self, peer_uuid: String) -> Result<()> {
        let existed = {
            let conn = self.lock_db()?;
            let existed = sync_store::remove_peer(&conn, &peer_uuid)?;
            // Their contribution to merged lifetime counters goes too.
            let _ = sync_store::remove_remote_stats(&conn, &peer_uuid);
            existed
        };
        if !existed {
            return Err(anyhow!("that device is not paired"));
        }
        {
            let mut status = self
                .inner
                .status
                .lock()
                .map_err(|_| anyhow!("status lock poisoned"))?;
            status.remove(&peer_uuid);
        }
        // Best-effort unpair notification so the peer forgets us too.
        let manager = self.clone();
        let uuid = peer_uuid.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(err) = manager.send_unpair(&uuid).await {
                log::info!("sync: unpair notification to {uuid} failed (harmless): {err:#}");
            }
        });
        self.inner.emit_devices_changed();
        Ok(())
    }

    async fn send_unpair(&self, peer_uuid: &str) -> Result<()> {
        let addr = self
            .addr_for_peer(peer_uuid)?
            .ok_or_else(|| anyhow!("device not visible"))?;
        let (cert, key) = {
            let guard = self.inner.identity.read().expect("identity lock");
            let identity = guard.as_ref().ok_or_else(|| anyhow!("sync unavailable"))?;
            (identity.cert_der().clone(), identity.tls_key())
        };
        let connector = transport::tls_connector(transport::client_config(cert, key)?);
        let tcp = tokio::time::timeout(CONNECT_TIMEOUT, tokio::net::TcpStream::connect(addr))
            .await
            .map_err(|_| anyhow!("timeout"))??;
        let mut tls = tokio::time::timeout(
            CONNECT_TIMEOUT,
            connector.connect(transport::server_name_for(peer_uuid), tcp),
        )
        .await
        .map_err(|_| anyhow!("tls timeout"))??;
        let my_uuid = self.device_info().uuid;
        send_message(
            &mut tls,
            &Message::Unpair {
                device_uuid: my_uuid,
            },
        )
        .await?;
        Ok(())
    }

    fn addr_for_peer(&self, peer_uuid: &str) -> Result<Option<SocketAddr>> {
        Ok(self.addr_candidates_for_peer(peer_uuid)?.into_iter().next())
    }

    fn addr_candidates_for_peer(&self, peer_uuid: &str) -> Result<Vec<SocketAddr>> {
        let discovered = self
            .inner
            .discovered
            .lock()
            .map_err(|_| anyhow!("discovery lock poisoned"))?;
        Ok(discovered
            .get(peer_uuid)
            .map(|d| connection_candidates(&d.addresses, d.port, peer_uuid))
            .unwrap_or_default())
    }

    /// Manual "Sync now". `None` syncs every discovered paired peer.
    pub async fn sync_now(&self, peer_uuid: Option<String>) -> Result<()> {
        match peer_uuid {
            Some(uuid) => {
                let manager = self.clone();
                tauri::async_runtime::spawn(async move {
                    manager.sync_to_peer(&uuid).await;
                });
            }
            None => {
                let uuids: Vec<String> = self
                    .inner
                    .discovered
                    .lock()
                    .map_err(|_| anyhow!("discovery lock poisoned"))?
                    .keys()
                    .cloned()
                    .collect();
                let manager = self.clone();
                tauri::async_runtime::spawn(async move {
                    for uuid in uuids {
                        manager.sync_to_peer(&uuid).await;
                    }
                });
            }
        }
        Ok(())
    }

    pub fn set_device_name(&self, name: String) -> Result<()> {
        let name = name.trim().chars().take(60).collect::<String>();
        if name.is_empty() {
            return Err(anyhow!("Device name cannot be empty"));
        }
        {
            let guard = self.inner.identity.read().expect("identity lock");
            let identity = guard.as_ref().ok_or_else(|| anyhow!("sync unavailable"))?;
            let mut updated = (**identity).clone();
            updated.name = name.clone();
            drop(guard);
            let mut guard = self.inner.identity.write().expect("identity lock");
            *guard = Some(Arc::new(updated));
        }
        {
            let conn = self.lock_db()?;
            sync_store::ensure_self_identity(&conn, &self.device_info().uuid, &name)?;
        }
        // Re-advertise with the new name (best-effort; the next restart also fixes it).
        let manager = self.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(err) = manager.start_discovery().await {
                log::warn!("sync: re-advertise after rename failed: {err:#}");
            }
        });
        self.inner.emit_devices_changed();
        Ok(())
    }

    /// Called by the settings layer after a synced setting changes locally.
    pub fn mark_dirty(&self) {
        self.inner.dirty.store(true, Ordering::Relaxed);
    }

    async fn monitor_tick(&self) {
        let dirty = self.inner.dirty.swap(false, Ordering::Relaxed);
        let now = Instant::now();
        let targets: Vec<String> = {
            let discovered = match self.inner.discovered.lock() {
                Ok(map) => map.values().cloned().collect::<Vec<_>>(),
                Err(_) => return,
            };
            // No possible targets: avoid opening/querying the peer database
            // every five seconds on machines with no discovered devices.
            if discovered.is_empty() {
                return;
            }
            let paired = paired_set(conn_peers(&self.inner.db));
            let backoff = match self.inner.backoff.lock() {
                Ok(b) => b,
                Err(_) => return,
            };
            discovered
                .into_iter()
                .filter(|d| paired.contains(&d.uuid))
                .filter(|d| should_auto_initiate(&self.device_info().uuid, &d.uuid))
                .filter(|d| match backoff.get(&d.uuid) {
                    Some(entry) => now >= entry.next_attempt || dirty,
                    None => true,
                })
                .map(|d| d.uuid)
                .collect()
        };
        for uuid in targets {
            let manager = self.clone();
            tauri::async_runtime::spawn(async move {
                manager.sync_to_peer(&uuid).await;
            });
        }
    }

    /// Runs one sync session with a paired peer (if discovered and idle).
    pub async fn sync_to_peer(&self, peer_uuid: &str) {
        if peer_uuid == self.device_info().uuid {
            return;
        }
        {
            let mut sessions = match self.inner.sessions.lock() {
                Ok(s) => s,
                Err(_) => return,
            };
            if !sessions.insert(peer_uuid.to_string()) {
                return; // session already running
            }
        }
        let _guard = SessionGuard(self.inner.clone(), peer_uuid.to_string());

        let addrs = self.addr_candidates_for_peer(peer_uuid).unwrap_or_default();
        if addrs.is_empty() {
            return; // not visible right now; discovery will retrigger us
        }
        let Ok(peer) = (|| {
            let conn = self.lock_db()?;
            sync_store::get_peer(&conn, peer_uuid)?.ok_or_else(|| anyhow!("not paired"))
        })() else {
            return;
        };

        self.set_status(peer_uuid, PeerState::Connecting, None);
        let mut result = Err(anyhow!("no connection candidates"));
        for addr in addrs {
            match self.run_client_session(&peer, addr).await {
                Ok(summary) => {
                    result = Ok(summary);
                    break;
                }
                Err(err) => result = Err(err),
            }
        }
        match result {
            Ok(summary) => {
                {
                    let conn = match self.lock_db() {
                        Ok(conn) => conn,
                        Err(_) => return,
                    };
                    let _ = sync_store::mark_peer_synced(&conn, peer_uuid, 0);
                    let _ = sync_store::compact_log(&conn);
                }
                self.reset_backoff(peer_uuid);
                self.set_status(peer_uuid, PeerState::Synced, None);
                if summary.applied.applied > 0 || summary.settings_applied > 0 {
                    self.inner
                        .app
                        .emit(
                            "verenu:sync-data-changed",
                            serde_json::json!({ "tables": summary.applied.touched_tables() }),
                        )
                        .ok();
                }
            }
            Err(err) => {
                let message = format!("{err:#}");
                log::warn!("sync: session with {peer_uuid} failed: {message}");
                {
                    let conn = match self.lock_db() {
                        Ok(conn) => conn,
                        Err(_) => return,
                    };
                    let _ = sync_store::mark_peer_error(&conn, peer_uuid, &message);
                }
                self.bump_backoff(peer_uuid);
                self.set_status(peer_uuid, PeerState::Error, Some(&message));
                self.inner
                    .app
                    .emit(
                        "verenu:sync-status",
                        serde_json::json!({
                            "uuid": peer_uuid,
                            "state": "error",
                            "error": message,
                        }),
                    )
                    .ok();
            }
        }
    }

    async fn run_client_session(
        &self,
        peer: &SyncPeer,
        addr: SocketAddr,
    ) -> Result<engine::SessionSummary> {
        let (cert, key) = {
            let guard = self.inner.identity.read().expect("identity lock");
            let identity = guard.as_ref().ok_or_else(|| anyhow!("sync unavailable"))?;
            (identity.cert_der().clone(), identity.tls_key())
        };
        let connector = transport::tls_connector(transport::client_config(cert, key)?);
        let tcp = tokio::time::timeout(CONNECT_TIMEOUT, tokio::net::TcpStream::connect(addr))
            .await
            .map_err(|_| anyhow!("connection to {} timed out", peer.name))?
            .map_err(|e| anyhow!("could not reach {}: {e}", peer.name))?;
        let mut tls = tokio::time::timeout(
            CONNECT_TIMEOUT,
            connector.connect(transport::server_name_for(&peer.device_uuid), tcp),
        )
        .await
        .map_err(|_| anyhow!("TLS handshake with {} timed out", peer.name))?
        .map_err(|e| anyhow!("TLS handshake with {} failed: {e}", peer.name))?;

        // Authenticate: the peer's certificate fingerprint must match the pin
        // from pairing time.
        let presented = transport::peer_fingerprint(
            tls.get_ref()
                .1
                .peer_certificates()
                .ok_or_else(|| anyhow!("peer presented no certificate"))?,
        )?;
        if presented != peer.cert_fp {
            return Err(anyhow!(
                "{}'s identity changed since pairing - remove and re-pair the device",
                peer.name
            ));
        }

        let host = ManagerHost::new(&self.inner);
        self.set_status(&peer.device_uuid, PeerState::Syncing, None);
        // A stalled peer must not hold the session slot forever.
        let summary = tokio::time::timeout(
            Duration::from_secs(600),
            engine::run_session(&self.inner.db, &host, &mut tls, true, peer),
        )
        .await
        .map_err(|_| anyhow!("sync session with {} timed out", peer.name))??;
        Ok(summary)
    }

    fn set_status(&self, peer_uuid: &str, state: PeerState, error: Option<&str>) {
        if let Ok(mut status) = self.inner.status.lock() {
            status.insert(
                peer_uuid.to_string(),
                PeerStatus {
                    state: state.clone(),
                    error: error.map(|e| e.to_string()),
                },
            );
        }
        self.inner
            .app
            .emit(
                "verenu:sync-status",
                serde_json::json!({
                    "uuid": peer_uuid,
                    "state": state_string(state),
                    "error": error,
                }),
            )
            .ok();
    }

    fn reset_backoff(&self, peer_uuid: &str) {
        if let Ok(mut backoff) = self.inner.backoff.lock() {
            backoff.insert(
                peer_uuid.to_string(),
                Backoff {
                    failures: 0,
                    next_attempt: Instant::now() + Duration::from_secs(600),
                },
            );
        }
    }

    fn bump_backoff(&self, peer_uuid: &str) {
        let mut backoff = match self.inner.backoff.lock() {
            Ok(b) => b,
            Err(_) => return,
        };
        let entry = backoff.entry(peer_uuid.to_string()).or_insert(Backoff {
            failures: 0,
            next_attempt: Instant::now(),
        });
        entry.failures = entry.failures.saturating_add(1);
        let secs = (30u64)
            .saturating_mul(1u64 << entry.failures.saturating_sub(1).min(5))
            .min(MAX_BACKOFF.as_secs());
        entry.next_attempt = Instant::now() + Duration::from_secs(secs);
    }
}

/// RAII guard removing the peer from the active-session set.
struct SessionGuard(Arc<Inner>, String);

impl Drop for SessionGuard {
    fn drop(&mut self) {
        if let Ok(mut sessions) = self.0.sessions.lock() {
            sessions.remove(&self.1);
        }
    }
}

struct PairingInProgressGuard<'a>(&'a AtomicBool);

impl Drop for PairingInProgressGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl Inner {
    /// Tells the frontend the discovered/paired device lists may have changed.
    pub(crate) fn emit_devices_changed(&self) {
        let _ = self.app.emit("verenu:sync-devices-changed", ());
    }

    fn set_pairing_status(&self, status: PairingStatus) {
        if let Ok(mut pairing) = self.pairing_status.lock() {
            *pairing = Some(status);
        }
        self.emit_devices_changed();
    }

    fn update_pairing_phase(&self, generation: u64, phase: &'static str) {
        if let Ok(mut pairing) = self.pairing_status.lock() {
            if let Some(status) = pairing.as_mut() {
                if status.generation == generation {
                    status.phase = phase;
                    status.error = None;
                }
            }
        }
        self.emit_devices_changed();
    }

    fn fail_pairing(&self, generation: u64, error: String) {
        if let Ok(mut pairing) = self.pairing_status.lock() {
            if let Some(status) = pairing.as_mut() {
                if status.generation == generation {
                    status.phase = "failed";
                    status.error = Some(error);
                }
            }
        }
        self.emit_devices_changed();
    }

    fn clear_pairing_status(&self, generation: u64) {
        if let Ok(mut pairing) = self.pairing_status.lock() {
            if pairing
                .as_ref()
                .is_some_and(|status| status.generation == generation)
            {
                *pairing = None;
            }
        }
        self.emit_devices_changed();
    }
}

pub(crate) fn connection_candidates(
    addresses: &[String],
    advertised_port: u16,
    peer_uuid: &str,
) -> Vec<SocketAddr> {
    let stable_port = listener_port_for_uuid(peer_uuid);
    let mut candidates = Vec::new();
    for address in addresses {
        let Ok(parsed) = address.parse::<SocketAddr>() else {
            let Ok(ip) = address.parse::<std::net::IpAddr>() else {
                continue;
            };
            for port in [stable_port, advertised_port] {
                let candidate = SocketAddr::new(ip, port);
                if !candidates.contains(&candidate) {
                    candidates.push(candidate);
                }
            }
            continue;
        };
        for port in [stable_port, advertised_port] {
            let candidate = SocketAddr::new(parsed.ip(), port);
            if !candidates.contains(&candidate) {
                candidates.push(candidate);
            }
        }
    }
    candidates
}

fn now_ms_u64() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn state_string(state: PeerState) -> String {
    match state {
        PeerState::Offline => "offline".to_string(),
        PeerState::Connecting => "connecting".to_string(),
        PeerState::Syncing => "syncing".to_string(),
        PeerState::Synced => "synced".to_string(),
        PeerState::Error => "error".to_string(),
    }
}

fn paired_set(peers: Vec<SyncPeer>) -> HashSet<String> {
    peers.into_iter().map(|p| p.device_uuid).collect()
}

fn conn_peers(db: &DbHandle) -> Vec<SyncPeer> {
    let conn = match db.lock() {
        Ok(conn) => conn,
        Err(_) => return Vec::new(),
    };
    sync_store::list_peers(&conn).unwrap_or_default()
}

fn stored_device_name(conn: &rusqlite::Connection) -> Result<Option<String>> {
    let name: Option<String> = conn
        .query_row("SELECT name FROM sync_identity LIMIT 1", [], |r| r.get(0))
        .optional()?;
    Ok(name)
}

// ---- discovery ----

/// A peer advertised itself on the LAN. Update the discovered cache, tell the
/// UI, and - if it's a paired device we're not already syncing with and its
/// backoff has elapsed - start a sync session right away.
fn handle_resolved(inner: &Arc<Inner>, info: mdns_sd::ResolvedService) {
    let uuid = info
        .get_property_val_str("uuid")
        .map(str::to_string)
        .or_else(|| {
            let instance_name = info.get_fullname().split('.').next().unwrap_or("");
            let instance = instance_name
                .strip_prefix("verenu-")
                .unwrap_or(instance_name);
            (!instance.is_empty()).then(|| instance.to_string())
        });
    let Some(uuid) = uuid else { return };
    let self_uuid = {
        let guard = inner.identity.read().expect("identity lock");
        match guard.as_ref() {
            Some(identity) => identity.uuid.clone(),
            None => return,
        }
    };
    if uuid == self_uuid || uuid.is_empty() {
        return;
    }
    let name = info
        .get_property_val_str("name")
        .unwrap_or("Verenu device")
        .to_string();
    let port = info.get_port();
    let addresses: Vec<String> = info
        .get_addresses()
        .iter()
        .map(|scoped| SocketAddr::new(scoped.to_ip_addr(), port).to_string())
        .collect();

    let changed = {
        let mut map = match inner.discovered.lock() {
            Ok(map) => map,
            Err(_) => return,
        };
        match map.get(&uuid) {
            Some(existing) => {
                let changed = existing.name != name
                    || existing.port != port
                    || existing.addresses != addresses;
                let entry = map.get_mut(&uuid).expect("checked above");
                entry.name = name.clone();
                entry.port = port;
                entry.addresses = addresses.clone();
                entry.last_seen_ms = now_ms_u64();
                changed
            }
            None => {
                map.insert(
                    uuid.clone(),
                    DiscoveredDevice {
                        uuid: uuid.clone(),
                        name: name.clone(),
                        addresses: addresses.clone(),
                        port,
                        last_seen_ms: now_ms_u64(),
                    },
                );
                true
            }
        }
    };
    if changed {
        inner.emit_devices_changed();
    }

    // Auto-sync on appearance, respecting backoff and one-session-per-peer.
    let paired = paired_set(conn_peers(&inner.db));
    if !paired.contains(&uuid) || !should_auto_initiate(&self_uuid, &uuid) {
        return;
    }
    let due = inner
        .backoff
        .lock()
        .map(|backoff| match backoff.get(&uuid) {
            Some(entry) => Instant::now() >= entry.next_attempt,
            None => true,
        })
        .unwrap_or(false);
    if !due {
        return;
    }
    let inner = inner.clone();
    let uuid = uuid.clone();
    tauri::async_runtime::spawn(async move {
        SyncManager { inner }.sync_to_peer(&uuid).await;
    });
}

// ---- incoming connection handling ----

async fn handle_connection(inner: Arc<Inner>, mut tls: TlsStream<tokio::net::TcpStream>) {
    let peer_fp = match tls
        .get_ref()
        .1
        .peer_certificates()
        .ok_or_else(|| anyhow!("no certificate"))
        .and_then(|certs| transport::peer_fingerprint(certs))
    {
        Ok(fp) => fp,
        Err(err) => {
            log::debug!("sync: incoming connection without certificate: {err}");
            return;
        }
    };
    let first = match tokio::time::timeout(CONNECT_TIMEOUT, read_message(&mut tls)).await {
        Ok(Ok(message)) => message,
        _ => return,
    };
    match first {
        Message::PairRequest {
            device_uuid,
            device_name,
            protocol,
            spake_msg,
        } => {
            handle_incoming_pairing(inner, tls, device_uuid, device_name, protocol, spake_msg)
                .await;
        }
        Message::Hello(hello) => {
            handle_sync_hello(inner, tls, hello, peer_fp).await;
        }
        Message::Unpair { device_uuid } => {
            // Only honor unpair requests from the pinned certificate of the
            // device being removed - anything else is an impostor.
            let matches = (|| {
                let conn = inner.db.lock().ok()?;
                sync_store::get_peer(&conn, &device_uuid)
                    .ok()
                    .flatten()
                    .map(|peer| peer.cert_fp == peer_fp)
            })()
            .unwrap_or(false);
            if matches {
                if let Ok(conn) = inner.db.lock() {
                    let _ = sync_store::remove_peer(&conn, &device_uuid);
                }
                inner.emit_devices_changed();
                log::info!("sync: removed by peer {device_uuid}");
            }
        }
        _ => {
            let _ = send_message(
                &mut tls,
                &Message::Error {
                    message: "unexpected first message".to_string(),
                },
            )
            .await;
        }
    }
}

async fn handle_incoming_pairing(
    inner: Arc<Inner>,
    mut tls: TlsStream<tokio::net::TcpStream>,
    peer_uuid: String,
    peer_name: String,
    protocol: u32,
    spake_msg: Vec<u8>,
) {
    if protocol != PROTOCOL_VERSION {
        let _ = send_message(
            &mut tls,
            &Message::PairReject {
                reason: format!("protocol version {protocol} not supported"),
            },
        )
        .await;
        return;
    }
    let generation;
    {
        let mut pending = inner.pending.lock().await;
        if inner.pairing_in_progress.load(Ordering::Acquire) || pending.is_some() {
            let _ = send_message(&mut tls, &Message::PairBusy).await;
            return;
        }
        generation = inner.pairing_generation.fetch_add(1, Ordering::Relaxed) + 1;
        *pending = Some(PendingPairing::Incoming {
            peer_uuid: peer_uuid.clone(),
            peer_name: peer_name.clone(),
            spake_msg,
            stream: Box::new(tls),
            created: Instant::now(),
            generation,
        });
    }
    inner.set_pairing_status(PairingStatus {
        kind: "incoming",
        phase: "awaiting_code",
        peer_uuid: peer_uuid.clone(),
        peer_name: peer_name.clone(),
        code: None,
        error: None,
        generation,
    });
    log::info!("sync: pairing request received from {peer_name}");
    let _ = inner.app.emit(
        "verenu:sync-pair-request",
        serde_json::json!({ "uuid": peer_uuid, "name": peer_name }),
    );
    inner.emit_devices_changed();

    // Prompt watchdog: if nobody responds before the lifetime, drop the
    // request (closing the held stream) so the next pairing isn't blocked.
    let watch_inner = inner.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(PAIRING_PROMPT_LIFETIME).await;
        let mut pending = watch_inner.pending.lock().await;
        if let Some(PendingPairing::Incoming { created, .. }) = pending.as_ref() {
            if created.elapsed() >= PAIRING_PROMPT_LIFETIME - Duration::from_secs(2)
                && watch_inner.pairing_generation.load(Ordering::Relaxed) == generation
            {
                *pending = None;
                watch_inner.clear_pairing_status(generation);
                watch_inner.emit_devices_changed();
            }
        }
    });
}

async fn handle_sync_hello(
    inner: Arc<Inner>,
    mut tls: TlsStream<tokio::net::TcpStream>,
    hello: Hello,
    peer_fp: String,
) {
    // Authenticate before anything else: the Hello's uuid must map to a paired
    // device whose pinned fingerprint matches the presented certificate.
    let peer = (|| {
        let conn = inner.db.lock().ok()?;
        sync_store::get_peer(&conn, &hello.device_uuid)
            .ok()
            .flatten()
    })();
    let Some(peer) = peer else {
        let _ = send_message(
            &mut tls,
            &Message::Error {
                message: "not paired with this device".to_string(),
            },
        )
        .await;
        return;
    };
    if peer.cert_fp != peer_fp {
        log::warn!(
            "sync: fingerprint mismatch for {} - rejecting connection",
            hello.device_uuid
        );
        let _ = send_message(
            &mut tls,
            &Message::Error {
                message: "certificate does not match the paired device".to_string(),
            },
        )
        .await;
        return;
    }
    if hello.protocol != PROTOCOL_VERSION {
        let _ = send_message(
            &mut tls,
            &Message::Error {
                message: format!(
                    "peer speaks sync protocol v{}, this device speaks v{PROTOCOL_VERSION}",
                    hello.protocol
                ),
            },
        )
        .await;
        return;
    }
    // Reserve the session before acknowledging it. Otherwise a duplicate
    // connection can receive HelloAck and then fail at the first session read.
    let session_available = {
        let mut sessions = inner.sessions.lock().expect("sessions lock");
        sessions.insert(hello.device_uuid.clone())
    };
    if !session_available {
        let _ = send_message(
            &mut tls,
            &Message::Error {
                message: "a sync session with this device is already running".to_string(),
            },
        )
        .await;
        return;
    }
    let _session_guard = SessionGuard(inner.clone(), hello.device_uuid.clone());
    let host = ManagerHost::new(&inner);
    // A stalled peer must not hold the session slot forever.
    let result = match tokio::time::timeout(
        Duration::from_secs(600),
        engine::run_session_after_hello(&inner.db, &host, &mut tls, false, &peer, Some(hello)),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => Err(anyhow!("sync session with {} timed out", peer.name)),
    };
    match result {
        Ok(summary) => {
            if let Ok(conn) = inner.db.lock() {
                let _ = sync_store::mark_peer_synced(&conn, &peer.device_uuid, 0);
                let _ = sync_store::compact_log(&conn);
            }
            if summary.applied.applied > 0 || summary.settings_applied > 0 {
                let _ = inner.app.emit(
                    "verenu:sync-data-changed",
                    serde_json::json!({ "tables": summary.applied.touched_tables() }),
                );
            }
        }
        Err(err) => {
            log::warn!("sync: incoming session with {} failed: {err:#}", peer.name);
            if let Ok(conn) = inner.db.lock() {
                let _ = sync_store::mark_peer_error(&conn, &peer.device_uuid, &format!("{err:#}"));
            }
        }
    }
}

// ---- SyncHost implementation over the real app ----

pub(crate) struct ManagerHost {
    db: DbHandle,
    settings: SettingsHandle,
    app: AppHandle,
    uuid: String,
    installed_apps: Vec<crate::system::apps::InstalledApp>,
}

impl ManagerHost {
    pub fn new(inner: &Arc<Inner>) -> Self {
        let uuid = inner
            .identity
            .read()
            .ok()
            .and_then(|identity| identity.as_ref().map(|identity| identity.uuid.clone()))
            .unwrap_or_default();
        Self {
            db: inner.db.clone(),
            settings: store::settings_handle(&inner.app).unwrap_or_else(|_| {
                // SettingsHandle::open failing here is practically impossible
                // (the same file opened fine at startup); fall back to a fresh
                // handle so sync continues with in-memory settings.
                store::SettingsHandle::open(&inner.app).expect("settings handle")
            }),
            app: inner.app.clone(),
            uuid,
            installed_apps: crate::system::apps::list_installed_apps(),
        }
    }
}

impl SyncHost for ManagerHost {
    fn device_uuid(&self) -> String {
        self.uuid.clone()
    }

    fn device_name(&self) -> String {
        // Read through the manager's identity when reachable; fall back to the
        // DB row (kept in sync by set_device_name).
        self.app
            .try_state::<SyncManager>()
            .map(|manager| manager.device_info().name)
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "This device".to_string())
    }

    fn app_version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    fn settings_payload(&self) -> anyhow::Result<Vec<super::protocol::SettingRecord>> {
        let stamps: HashMap<String, (i64, String)> = (|| {
            let conn = self.db.lock().ok()?;
            Some(
                sync_store::list_setting_stamps(&conn)
                    .ok()?
                    .into_iter()
                    .map(|(key, stamp)| (key, (stamp.ts_ms, stamp.origin)))
                    .collect(),
            )
        })()
        .unwrap_or_default();
        let mut records = Vec::new();
        for key in engine::SYNCABLE_SETTINGS {
            if let Some(value) = self.settings.get(key) {
                let (ts_ms, origin) = stamps.get(*key).cloned().unwrap_or((0, String::new()));
                records.push(super::protocol::SettingRecord {
                    key: key.to_string(),
                    value,
                    ts_ms,
                    origin,
                });
            }
        }
        Ok(records)
    }

    fn apply_remote_setting(&self, key: &str, value: &serde_json::Value) -> Result<(), String> {
        // Values come from a trusted peer, but they still go through the same
        // validation the local save path uses.
        validate_setting(key, value)?;
        if key == store::CONTEXTUAL_FORMATTING {
            self.settings.set_many([
                (store::CONTEXTUAL_FORMATTING, value.clone()),
                (store::CONTEXTUAL_CAPS, value.clone()),
                (store::AUTO_SPACING, value.clone()),
            ])?;
            self.settings.save()?;
        } else {
            self.settings.save_value(key, value.clone())?;
        }
        // Side effects that keep the running app consistent with the new value.
        if crate::app_tray::setting_updates_runtime_icons(key) {
            crate::app_tray::apply_runtime_icons(&self.app, None);
        }
        if key == store::SOUND_EFFECTS_VOLUME {
            if let Some(volume) = value.as_f64() {
                crate::media::sound::set_volume((volume as f32) / 100.0);
            }
        }
        Ok(())
    }

    fn resolve_app_target(&self, source: &str) -> Option<String> {
        closest_installed_app(source, &self.installed_apps).map(|app| app.exe.clone())
    }

    fn resolve_app_target_with_metadata(
        &self,
        source: &str,
        app_name: Option<&str>,
        developer: Option<&str>,
    ) -> Option<(String, Option<String>, Option<String>)> {
        crate::system::apps::closest_installed_app(
            source,
            app_name,
            developer,
            &self.installed_apps,
        )
        .map(|app| {
            (
                app.exe.clone(),
                Some(app.name.clone()),
                app.developer.clone(),
            )
        })
    }
}

pub(crate) fn closest_installed_app<'a>(
    source: &str,
    apps: &'a [crate::system::apps::InstalledApp],
) -> Option<&'a crate::system::apps::InstalledApp> {
    crate::system::apps::closest_installed_app(source, None, None, apps)
}
