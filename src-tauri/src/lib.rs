use tauri::{path::BaseDirectory, Manager};
use tauri_plugin_updater::UpdaterExt;

use crate::prelude::NaiveLogger;

mod options;
mod prelude;
mod teleporter_config;
mod tsh;
mod tsh_version;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: String, handle: tauri::AppHandle) -> String {
    log::info!("Tauri is awesome! 2");
    let res = NaiveLogger::init("/Users/paolo/w/logs", "teleporter.log");
    if res.is_err() {
        return format!("Error initializing logger: {:?}", res.err());
    }

    let resource_path = handle
        .path()
        .resolve("bin/tsh.app/Contents/MacOS/tsh", BaseDirectory::Resource)
        .expect("error getting tsh resource");

    let res = tsh::login(resource_path.to_str().unwrap());
    if res.is_err() {
        return format!("Error logging in with tsh: {:?}", res.err());
    }

    let res = tsh::get_cfg(resource_path.to_str().unwrap());
    if res.is_err() {
        return format!("Error getting tsh config: {:?}", res.err());
    }

    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(tauri_plugin_log::log::LevelFilter::Info)
                .build(),
        )
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                update(handle).await.unwrap();
            });
            Ok(())
        })
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(|app, args, cwd| {}))
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

async fn update(app: tauri::AppHandle) -> tauri_plugin_updater::Result<()> {
    if let Some(update) = app.updater()?.check().await? {
        let mut downloaded = 0;
        update
            .download_and_install(
                |chunk_length, content_length| {
                    downloaded += chunk_length;
                    println!("downloaded {downloaded} from {content_length:?}");
                },
                || {
                    println!("download finished");
                },
            )
            .await?;

        println!("update installed");
        app.restart();
    }

    println!("no update available");
    log::info!("Tauri is awesome!");

    Ok(())
}
