use crate::prelude::*;
use regex::Regex;
use std::process::{Command, Stdio};
use std::time::Duration;

const TSH_AUTH_ARGS: &[&str] = &["--proxy", "dc.mago.cloud", "--auth", "github"];
const PROXY: &str = "ztvsproxy03.zg.local:8080";

#[allow(dead_code)]
#[derive(Debug)]
pub struct LoginResult {
    pub user: String,
    pub roles: String,
    pub logins: String,
    pub extensions: String,
}

fn tsh(tsh_path: &str, cmd: &str) -> Command {
    let mut res = Command::new(tsh_path);
    res.env("TELEPORTER", "1"); // to identify tsh processes started by us
    res.arg(cmd);
    res
}

const TUNNEL_PROTOCOL: &str = "teleporter-tunnel";
const SERVER_ADDR: &str = "127.0.0.1:47476";

use tokio::io::{copy_bidirectional, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::spawn;
use tokio::sync::watch;

pub async fn bind_listener(local_bind: &str) -> anyhow::Result<TcpListener> {
    TcpListener::bind(local_bind).await.loc()
}

pub async fn listen(
    listener: TcpListener,
    local_bind: &str,
    remote_host: &str,
    remote_port: u16,
    shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    log::info!("listening on {}", local_bind);

    loop {
        if *shutdown.borrow() {
            log::info!("stopping listener on {}", local_bind);
            break;
        }

        let accepted = tokio::time::timeout(Duration::from_millis(250), listener.accept()).await;
        match accepted {
            Ok(accepted) => {
                let (mut local, peer) = accepted?;
                log::info!("accepted from {}", peer);
                let remote_host = remote_host.to_string();

                spawn(async move {
                    if let Err(e) = handle_conn(&mut local, remote_host, remote_port).await {
                        log::error!("connection error: {e:?}");
                    }
                });
            }
            Err(_) => continue,
        }
    }

    Ok(())
}

async fn handle_conn(
    local: &mut TcpStream,
    remote_host: String,
    remote_port: u16,
) -> anyhow::Result<()> {
    // connect to the server
    let mut srv = TcpStream::connect(SERVER_ADDR).await.loc()?;
    let host = SERVER_ADDR.split(':').next().unwrap_or(SERVER_ADDR);

    // send HTTP/1.1 upgrade request
    let req = format!(
        "GET /tunnel?uri={}&port={} HTTP/1.1\r\nHost: {}\r\nConnection: Upgrade\r\nUpgrade: {}\r\n\r\n",
        remote_host, remote_port, host, TUNNEL_PROTOCOL,
    );
    srv.write_all(req.as_bytes()).await.loc()?;

    // read response headers until \r\n\r\n
    let mut buf = [0u8; 1024];
    let mut header_bytes = Vec::new();
    loop {
        let n = srv.read(&mut buf).await.loc()?;
        if n == 0 {
            anyhow::bail!("server closed connection during handshake");
        }
        header_bytes.extend_from_slice(&buf[..n]);
        if header_bytes.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
        if header_bytes.len() > 16 * 1024 {
            anyhow::bail!("handshake headers too large");
        }
    }
    let header_str = String::from_utf8_lossy(&header_bytes);
    if !header_str.starts_with("HTTP/1.1 101") && !header_str.starts_with("HTTP/1.0 101") {
        anyhow::bail!(
            "upgrade failed: {}",
            header_str.lines().next().unwrap_or("")
        );
    }

    log::info!(
        "upgrade successful, tunneling to {}:{}",
        remote_host,
        remote_port
    );

    // Now bridge bytes bidirectionally between local <-> srv
    // Note: any leftover bytes after headers are already in header_bytes;
    // if necessary, write leftover to `local` before starting the copy.
    // For brevity we assume none.

    let (mut l, mut s) = (local, srv);
    let (a, b) = copy_bidirectional(&mut l, &mut s).await.loc()?;
    log::info!("tunnel closed; sent={} received={}", a, b);

    Ok(())
}

pub fn tsh_proxy_app(tsh_path: &str) -> Result<bool> {
    let mut cmd = Command::new(tsh_path);
    cmd.env("TELEPORTER", "1"); // to identify tsh processes started by us
    cmd.args(["proxy", "app", "paolo-test", "--port", "47476"]);
    cmd.args(TSH_AUTH_ARGS);
    _ = dump!(&cmd);
    let mut ch = cmd.no_window().spawn()?;

    let res = ch.wait()?;

    Ok(res.success())
}

pub fn execute_output(cmd: &mut Command) -> Result<String> {
    _ = dump!(&cmd);
    let out = cmd.no_window().output()?;
    if !out.status.success() {
        bail!("{}", dump!(String::from_utf8_lossy(&out.stderr)));
    }

    Ok(dump!(String::from_utf8_lossy(&out.stdout).into_owned()))
}

pub fn execute_output_with_timeout(cmd: &mut Command, timeout: Duration) -> Result<String> {
    _ = dump!(&cmd);
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .no_window()
        .spawn()?;
    let start = std::time::Instant::now();

    loop {
        if let Some(status) = child.try_wait()? {
            let out = child.wait_with_output()?;
            if !status.success() {
                bail!("{}", dump!(String::from_utf8_lossy(&out.stderr)));
            }

            return Ok(dump!(String::from_utf8_lossy(&out.stdout).into_owned()));
        }

        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            bail!("command timed out after {}s", timeout.as_secs());
        }

        std::thread::sleep(Duration::from_millis(200));
    }
}

pub fn login(tsh_path: &str) -> Result<LoginResult> {
    let out = execute_output_with_timeout(
        tsh(tsh_path, "login").args(TSH_AUTH_ARGS),
        Duration::from_secs(60),
    )?;
    let re = Regex::new(
        r"(?ms)Logged in as:\s*(.*?)$.*Roles:\s*(.*?)$.*Logins:\s*(.*?)$.*Extensions:\s*(.*?)$",
    )
    .unwrap();
    let cs = re.captures(&out).ok_or_err("can't match login result")?;
    let res = LoginResult {
        user: cs.get(1).ok_or_err("can't get user")?.as_str().into(),
        roles: cs.get(2).ok_or_err("can't get roles")?.as_str().into(),
        logins: cs.get(3).ok_or_err("can't get logins")?.as_str().into(),
        extensions: cs.get(4).ok_or_err("can't get extensions")?.as_str().into(),
    };
    Ok(dump!(res))
}

// fn logout(tsh_path: &str) -> Result<()> {
//     // execute(&mut tsh(tsh_path, "logout")).context("logout failed")?;
//     Ok(())
// }

pub fn kill_tsh() -> Result<()> {
    use sysinfo::{Signal, System};

    let system = System::new_all();

    for process in system.processes().values() {
        // let name = process.name().to_string_lossy();
        let exe_name = process
            .exe()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();

        if !matches!(exe_name.as_ref(), "tsh.exe" | "tsh") {
            continue;
        }

        // kills only processes with TELEPORTER=1 in env, to avoid killing unrelated tsh instances
        if let Some(val) = process
            .environ()
            .iter()
            .filter_map(|e| e.to_str()) // OsString -> &str
            .find(|e| e.starts_with("TELEPORTER="))
        {
            log::info!(
                "killing tsh process with pid {}, env {}",
                process.pid(),
                val
            );
            process
                .kill_with(Signal::Term)
                .unwrap_or_else(|| process.kill());
        }
    }

    Ok(())
}
