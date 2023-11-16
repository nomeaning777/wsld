use std::process::Stdio;

use super::util::{connect_stream, either};

use anyhow::{anyhow, Context as _, Result};
use tokio::fs::read;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::process::Command;

enum SocketType {
    Default,
    Extra,
}
pub async fn handle_gpg_agent(mut stream: TcpStream) -> Result<()> {
    let mut socket_type = [0u8; 1];
    stream.read_exact(&mut socket_type).await?;

    let socket_type = match socket_type[0] {
        b'0' => SocketType::Default,
        b'1' => SocketType::Extra,
        _ => return Err(anyhow!("invalid socket type")),
    };

    let gpg_conf = Command::new("gpgconf.exe")
        .arg("--list-dir")
        .arg(match socket_type {
            SocketType::Default => "agent-socket",
            SocketType::Extra => "agent-extra-socket",
        })
        .stdout(Stdio::piped())
        .spawn()?;
    let output = gpg_conf.wait_with_output().await?;
    if !output.status.success() {
        return Err(anyhow!("gpgconf failure"));
    }

    // Start gpg-agent
    let run_gpg_agent = Command::new("gpg-connect-agent.exe")
        .arg("/bye")
        .spawn()?
        .wait()
        .await?;
    if !run_gpg_agent.success() {
        return Err(anyhow!("gpg-connect-agent failure"));
    }

    let content = read(std::str::from_utf8(&output.stdout)?.trim()).await?;
    for i in 0..content.len() {
        if content[i] == b'\n' {
            let port: u16 = std::str::from_utf8(&content[0..i])
                .context("non utf8 port number")?
                .parse()
                .context("invalid port number for gpg-agent")?;
            let (client_r, client_w) = stream.split();
            let mut server = TcpStream::connect(("127.0.0.1", port)).await?;
            server.set_nodelay(true)?;
            let (server_r, mut server_w) = server.split();
            server_w.write_all(&content[i + 1..]).await?;
            let a = connect_stream(client_r, server_w);
            let b = connect_stream(server_r, client_w);
            return Ok(either(a, b).await?);
        }
    }
    Err(anyhow!("invalid format of agent-socket"))
}
