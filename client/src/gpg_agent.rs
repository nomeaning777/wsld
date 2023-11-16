use super::config::GpgAgentConfig;
use super::util::{connect_stream, either};
use super::vmsocket::VmSocket;
use super::CONFIG;

use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tokio::io::AsyncWriteExt;
use tokio::net::{UnixListener, UnixStream};

enum SocketType {
    Default,
    Extra,
}

async fn handle_stream(mut stream: UnixStream, socket_type: SocketType) -> std::io::Result<()> {
    let mut server = VmSocket::connect(CONFIG.service_port).await?;
    server.write_all(b"gpga").await?;
    match socket_type {
        SocketType::Default => server.write_all(b"0").await?,
        SocketType::Extra => server.write_all(b"1").await?,
    }

    let (client_r, client_w) = stream.split();
    let (server_r, server_w) = server.split();
    let a = connect_stream(client_r, server_w);
    let b = connect_stream(server_r, client_w);
    either(a, b).await
}

pub async fn gpg_agent_forward(config: &'static GpgAgentConfig) -> std::io::Result<()> {
    // Remove existing socket
    let _ = std::fs::create_dir_all(Path::new(&config.gpg_agent_sock).parent().unwrap());
    let _ = std::fs::create_dir_all(Path::new(&config.gpg_agent_extra_sock).parent().unwrap());
    let _ = std::fs::remove_file(&config.gpg_agent_sock);
    let _ = std::fs::remove_file(&config.gpg_agent_extra_sock);

    let default_listener = UnixListener::bind(&config.gpg_agent_sock)?;
    let extra_listener = UnixListener::bind(&config.gpg_agent_extra_sock)?;
    let _ = std::fs::set_permissions(&config.gpg_agent_sock, Permissions::from_mode(0o600));
    let _ = std::fs::set_permissions(&config.gpg_agent_extra_sock, Permissions::from_mode(0o600));

    loop {
        tokio::select! {
            stream = default_listener.accept() => {
                let stream = stream?.0;
                tokio::task::spawn(async move {
                    if let Err(err) = handle_stream(stream, SocketType::Default).await {
                        eprintln!("Failed to transfer: {}", err);
                    }
                });
            }
            stream = extra_listener.accept() => {
                let stream = stream?.0;
                tokio::task::spawn(async move {
                    if let Err(err) = handle_stream(stream, SocketType::Extra).await {
                        eprintln!("Failed to transfer: {}", err);
                    }
                });
            }
        }
    }
}
