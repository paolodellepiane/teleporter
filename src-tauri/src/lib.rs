use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{path::BaseDirectory, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;
use tokio::sync::{watch, Mutex};

use crate::prelude::*;

mod prelude;
mod teleporter_config;
mod tsh;
mod tsh_version;

#[derive(Clone, Default)]
struct AppState {
    tunnel: Arc<Mutex<Option<TunnelSession>>>,
    tunnels: Arc<Mutex<Vec<TunnelRuntime>>>,
}

struct TunnelSession {
    shutdown_tx: watch::Sender<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct TunnelDefinition {
    local: u16,
    name: String,
    dest: String,
    remote: u16,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum TunnelStatus {
    Pending,
    Active,
    Stopped,
    Error,
}

#[derive(Clone, Debug, Serialize)]
struct TunnelRuntime {
    local: u16,
    name: String,
    dest: String,
    remote: u16,
    status: TunnelStatus,
    message: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct TunnelPlan {
    tunnels: Vec<TunnelDefinition>,
}

#[derive(Clone, Debug, Serialize)]
struct TunnelStatePayload {
    connected: bool,
    tunnels: Vec<TunnelRuntime>,
}

const TUNNEL_STATE_EVENT: &str = "tunnel-state";

#[cfg(target_os = "macos")]
fn set_exe_permissions(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Err(err) = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)) {
        log::warn!("can't set permissions for {path:?}: {err:?}");
    };
}

#[cfg(target_os = "windows")]
fn set_exe_permissions(path: &std::path::Path) {}

fn prepare_tsh(handle: &tauri::AppHandle) -> Result<std::path::PathBuf> {
    let resource_path = handle
        .path()
        .resolve("bin", BaseDirectory::Resource)
        .context("error resolving tsh resource path")?;

    let app_cache = handle
        .path()
        .app_cache_dir()
        .context("error resolving cache path")?
        .join("bin");

    let tsh_path = app_cache.join(if cfg!(target_os = "macos") {
        "tsh.app/Contents/MacOS/tsh"
    } else {
        "tsh.exe"
    });

    match tsh_needs_update(&app_cache, &resource_path, &tsh_path) {
        Ok(true) => {
            log::info!("tsh needs update, extracting new version");
            extract_tsh(&resource_path, &app_cache, &tsh_path)?;
        }
        Ok(false) => {}
        Err(err) => {
            log::error!("error checking tsh version: {err:?}");
            bail!("error checking tsh version: {err:?}");
        }
    }

    Ok(tsh_path)
}

fn tsh_needs_update(
    app_cache: &Path,
    resource_path: &Path,
    tsh_path: &Path,
) -> anyhow::Result<bool> {
    log::info!("checking tsh version");
    if !std::fs::exists(app_cache).loc()? || !std::fs::exists(tsh_path).loc()? {
        return Ok(true);
    }

    let cached_version = std::fs::read_to_string(app_cache.join("version")).loc()?;
    let new_version = std::fs::read_to_string(resource_path.join("version")).loc()?;
    log::info!("tsh version: cached {cached_version}, new: {new_version}");

    Ok(new_version.trim() != cached_version.trim())
}

fn extract_tsh(
    resource_path: &Path,
    app_cache: &Path,
    tsh_path: &Path,
) -> Result<(), anyhow::Error> {
    if std::fs::exists(app_cache)? {
        std::fs::remove_dir_all(app_cache).context("error clearing cache directory")?;
    }

    std::fs::create_dir_all(app_cache).context("error creating cache directory")?;
    log::info!("extracting tsh");
    sevenz_rust2::decompress_file(
        resource_path.join("tsh.7z").to_str().unwrap(),
        app_cache.to_str().unwrap(),
    )
    .context("error decompressing tsh")?;
    std::fs::copy(resource_path.join("version"), app_cache.join("version"))
        .context("error copying version file")?;

    set_exe_permissions(tsh_path);

    Ok(())
}

async fn setup_tunnel(
    handle: tauri::AppHandle,
    tsh_path: std::path::PathBuf,
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
    app_state: AppState,
) -> Result<()> {
    log::info!("setting up tsh tunnel");

    tsh::kill_tsh().context("Error killing tsh")?;

    let shutdown_tx_for_proxy = shutdown_tx.clone();
    let proxy = tauri::async_runtime::spawn_blocking(move || {
        let res = tsh::tsh_proxy_app(tsh_path.to_str().ok_or_err("invalid tsh path")?);
        let _ = shutdown_tx_for_proxy.send(true);
        res
    });

    let tunnel_plan = fetch_tunnel_plan_stub().await?;
    {
        let mut tunnels = app_state.tunnels.lock().await;
        *tunnels = tunnel_plan
            .tunnels
            .iter()
            .map(|tunnel| TunnelRuntime {
                local: tunnel.local,
                name: tunnel.name.clone(),
                dest: tunnel.dest.clone(),
                remote: tunnel.remote,
                status: TunnelStatus::Pending,
                message: "Waiting for listener".to_string(),
            })
            .collect();
    }
    emit_tunnel_state(&handle, &app_state, true).await;
    log::info!("loaded {} tunnels from stub api", tunnel_plan.tunnels.len());

    let listeners = tunnel_plan
        .tunnels
        .into_iter()
        .map(|tunnel| {
            let shutdown = shutdown_rx.clone();
            let handle = handle.clone();
            let app_state = app_state.clone();
            tauri::async_runtime::spawn(async move {
                let local_bind = format!("127.0.0.1:{}", tunnel.local);
                match tsh::bind_listener(&local_bind).await {
                    Ok(listener) => {
                        update_tunnel_runtime(
                            &handle,
                            &app_state,
                            tunnel.local,
                            TunnelStatus::Active,
                            format!("Forwarding to {}:{}", tunnel.dest, tunnel.remote),
                            true,
                        )
                        .await;

                        let listen_result = tsh::listen(
                            listener,
                            &local_bind,
                            &tunnel.dest,
                            tunnel.remote,
                            shutdown,
                        )
                        .await;

                        match listen_result {
                            Ok(()) => {
                                update_tunnel_runtime(
                                    &handle,
                                    &app_state,
                                    tunnel.local,
                                    TunnelStatus::Stopped,
                                    "Listener stopped".to_string(),
                                    true,
                                )
                                .await;
                                Ok(())
                            }
                            Err(err) => {
                                update_tunnel_runtime(
                                    &handle,
                                    &app_state,
                                    tunnel.local,
                                    TunnelStatus::Error,
                                    err.to_string(),
                                    true,
                                )
                                .await;
                                Err(err).context(format!("tunnel {} failed", tunnel.name))
                            }
                        }
                    }
                    Err(err) => {
                        update_tunnel_runtime(
                            &handle,
                            &app_state,
                            tunnel.local,
                            TunnelStatus::Error,
                            err.to_string(),
                            true,
                        )
                        .await;
                        Err(err).context(format!("tunnel {} failed to bind", tunnel.name))
                    }
                }
            })
        })
        .collect::<Vec<_>>();

    let proxy_error = match proxy.await {
        Ok(Ok(true)) => None,
        Ok(Ok(false)) => Some(anyhow::anyhow!("tsh proxy process exited with error")),
        Ok(Err(err)) => Some(err),
        Err(e) => Some(anyhow::anyhow!("task join failed: {:?}", e)),
    };
    let _ = shutdown_tx.send(true);

    for listener in listeners {
        listener
            .await
            .map_err(|e| anyhow::anyhow!("task join failed: {:?}", e))??;
    }

    if let Some(err) = proxy_error {
        return Err(err);
    }

    Ok(())
}

async fn fetch_tunnel_plan_stub() -> Result<TunnelPlan> {
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    serde_json::from_str(include_str!("../../sample.json")).context("error parsing stub tunnel api")
}

async fn tunnel_state_payload(app_state: &AppState, connected: bool) -> TunnelStatePayload {
    let tunnels = app_state.tunnels.lock().await.clone();
    TunnelStatePayload { connected, tunnels }
}

async fn emit_tunnel_state(handle: &tauri::AppHandle, app_state: &AppState, connected: bool) {
    let payload = tunnel_state_payload(app_state, connected).await;
    if let Err(err) = handle.emit(TUNNEL_STATE_EVENT, payload) {
        log::warn!("error emitting tunnel state: {err:?}");
    }
}

async fn update_tunnel_runtime(
    handle: &tauri::AppHandle,
    app_state: &AppState,
    local: u16,
    status: TunnelStatus,
    message: String,
    connected: bool,
) {
    {
        let mut tunnels = app_state.tunnels.lock().await;
        if let Some(tunnel) = tunnels.iter_mut().find(|tunnel| tunnel.local == local) {
            tunnel.status = status;
            tunnel.message = message;
        }
    }

    emit_tunnel_state(handle, app_state, connected).await;
}

#[tauri::command]
async fn connect_tunnel(handle: tauri::AppHandle) -> Result<String, String> {
    let state = handle.state::<AppState>();

    {
        if state.tunnel.lock().await.is_some() {
            return Ok("Already connected".to_string());
        }
    }

    let tsh_path = match prepare_tsh(&handle) {
        Ok(path) => path,
        Err(err) => {
            log::error!("Error preparing tsh: {:?}", err);
            return Err(format!("Error preparing tsh: {:?}", err));
        }
    };

    let tsh_path_str = match tsh_path.to_str() {
        Some(path) => path,
        None => return Err("Error preparing tsh: invalid tsh path".to_string()),
    };

    if let Err(err) = tsh::login(tsh_path_str) {
        log::error!("Error logging in with tsh: {:?}", err);
        return Err(format!("Error logging in with tsh: {:?}", err));
    }

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    {
        let mut guard = state.tunnel.lock().await;
        *guard = Some(TunnelSession {
            shutdown_tx: shutdown_tx.clone(),
        });
    }

    let tunnel_state = state.tunnel.clone();
    let app_state = state.inner().clone();
    let app_handle = handle.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(err) = setup_tunnel(
            app_handle.clone(),
            tsh_path,
            shutdown_tx,
            shutdown_rx,
            app_state.clone(),
        )
        .await
        {
            log::error!("Error setting up tsh tunnel: {:?}", err);
        }

        let mut guard = tunnel_state.lock().await;
        guard.take();

        let mut tunnels = app_state.tunnels.lock().await;
        tunnels.clear();
        drop(tunnels);
        emit_tunnel_state(&app_handle, &app_state, false).await;
    });

    emit_tunnel_state(&handle, &state.inner().clone(), true).await;
    Ok("Connected".to_string())
}

#[tauri::command]
async fn disconnect_tunnel(handle: tauri::AppHandle) -> Result<String, String> {
    let state = handle.state::<AppState>();
    let shutdown_tx = {
        let mut guard = state.tunnel.lock().await;
        match guard.take() {
            Some(session) => session.shutdown_tx,
            None => return Ok("Already disconnected".to_string()),
        }
    };

    let _ = shutdown_tx.send(true);
    state.tunnels.lock().await.clear();
    emit_tunnel_state(&handle, &state.inner().clone(), false).await;

    match tsh::kill_tsh() {
        Ok(()) => Ok("Disconnected".to_string()),
        Err(err) => {
            log::error!("Error disconnecting tsh: {:?}", err);
            Err(format!("Error disconnecting tsh: {:?}", err))
        }
    }
}

#[tauri::command]
async fn tunnel_status(handle: tauri::AppHandle) -> Result<bool, String> {
    let state = handle.state::<AppState>();
    let connected = state.tunnel.lock().await.is_some();
    Ok(connected)
}

#[tauri::command]
async fn tunnel_list(handle: tauri::AppHandle) -> Result<Vec<TunnelDefinition>, String> {
    let state = handle.state::<AppState>();
    let tunnels = state
        .tunnels
        .lock()
        .await
        .iter()
        .map(|tunnel| TunnelDefinition {
            local: tunnel.local,
            name: tunnel.name.clone(),
            dest: tunnel.dest.clone(),
            remote: tunnel.remote,
        })
        .collect();
    Ok(tunnels)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(tauri_plugin_log::log::LevelFilter::Info)
                .build(),
        )
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move { update(handle).await });
            Ok(())
        })
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(|_app, _args, _cwd| {}))
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            connect_tunnel,
            disconnect_tunnel,
            tunnel_status,
            tunnel_list
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|_app_handle, _event| match _event {
            tauri::RunEvent::Exit { .. } => {
                log::info!("exit, killing tsh if running");
                tsh::kill_tsh().ok();
            }
            _ => {}
        });
}

async fn update(app: tauri::AppHandle) {
    let u = app.updater().expect("can't get updater");
    match u.check().await {
        Ok(update) => {
            match update {
                Some(update) => {
                    log::info!("update available: {:?}", update.current_version);
                    let mut downloaded = 0;
                    let res = update
                        .download_and_install(
                            |chunk_length, content_length| {
                                downloaded += chunk_length;
                                println!("downloaded {downloaded} from {content_length:?}");
                            },
                            || {
                                println!("download finished");
                            },
                        )
                        .await;

                    if let Err(err) = res {
                        log::error!("error downloading and installing update: {err:?}");
                        return;
                    }

                    println!("update installed");
                    app.restart();
                }
                None => {
                    log::info!("no update available");
                    return;
                }
            };
        }
        Err(err) => {
            log::error!("error checking for update: {err:?}");
        }
    }
}
