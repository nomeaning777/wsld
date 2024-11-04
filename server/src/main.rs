// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod gpg_agent;
mod ssh_agent;
mod tcp;
mod time;
mod util;
mod vmcompute;
mod vmsocket;
mod x11;

use anyhow::{bail, Result};
use clap::Parser;
use config::Config;
use log::info;
use once_cell::sync::Lazy;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::RunEvent;
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use uuid::Uuid;
use vmsocket::VmSocket;

static CONFIG: Lazy<Config> = Lazy::new(Config::parse);

async fn handle_stream(mut stream: TcpStream) -> Result<()> {
    // Read the function code at the start of the stream for demultiplexing
    let func = {
        let mut buf = [0; 4];
        stream.read_exact(&mut buf).await?;
        buf
    };

    match &func {
        b"x11\0" => x11::handle_x11(stream).await.map_err(Into::into),
        b"time" => time::handle_time(stream).await.map_err(Into::into),
        b"tcp\0" => tcp::handle_tcp(stream).await.map_err(Into::into),
        b"ssha" => ssh_agent::handle_ssh_agent(stream)
            .await
            .map_err(Into::into),
        b"gpga" => gpg_agent::handle_gpg_agent(stream)
            .await
            .map_err(Into::into),
        b"noop" => Ok(()),
        _ => bail!("unknown function {:?}", func),
    }
}

async fn task(vmid: Uuid) -> Result<()> {
    let listener = VmSocket::bind(vmid, CONFIG.service_port).await?;

    loop {
        let stream = listener.accept().await?;

        tokio::task::spawn(async move {
            let result = handle_stream(stream).await;
            if let Err(err) = result {
                eprintln!("Error: {}", err);
            }
        });
    }
}

async fn server_main() {
    if CONFIG.daemon {
        let mut prev_vmid = None;
        let mut future: Option<tokio::task::JoinHandle<()>> = None;
        loop {
            let vmid = tokio::task::spawn_blocking(|| vmcompute::get_wsl_vmid().unwrap())
                .await
                .unwrap();
            if vmid != prev_vmid {
                if let Some(future) = future.take() {
                    future.abort();
                }
                prev_vmid = vmid;
                if let Some(vmid) = vmid {
                    future = Some(tokio::task::spawn(async move {
                        // Three chances, to avoid a race between get_wsl_vmid and spawn.
                        for _ in 0..3 {
                            if let Err(err) = task(vmid).await {
                                eprintln!("Failed to listen: {}", err);
                            }
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        }
                        std::process::exit(1);
                    }));
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    } else {
        let vmid = match CONFIG.vmid {
            Some(str) => str,
            None => vmcompute::get_wsl_vmid()
                .unwrap()
                .expect("WSL is not running"),
        };

        if let Err(err) = task(vmid).await {
            eprintln!("Failed to listen: {}", err);
            return;
        }
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            app.handle().plugin(tauri_plugin_autostart::init(
                tauri_plugin_autostart::MacosLauncher::LaunchAgent,
                Some(Vec::new()),
            ));
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&quit_i])?;

            let tray = TrayIconBuilder::new()
                .menu(&menu)
                .menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        info!("quit menu item was clicked");
                        app.exit(0);
                    }
                    _ => {
                        info!("menu item {:?} not handled", event.id);
                    }
                })
                .build(app)?;
            let ans = app
                .dialog()
                .message("File not found")
                .kind(MessageDialogKind::Error)
                .title("Warning")
                .blocking_show();
            tauri::async_runtime::spawn(server_main());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(move |app_handle, e| {
            // Keep the event loop running even if all windows are closed
            // This allow us to catch system tray events when there is no window
            if let RunEvent::ExitRequested { api, code, .. } = &e {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}
