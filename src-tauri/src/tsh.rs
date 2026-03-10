use crate::{prelude::*, teleporter_config};
use itertools::Itertools;
use regex::Regex;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

const TSH_AUTH_ARGS: &[&str] = &["--proxy", "dc.mago.cloud", "--auth", "github"];
const TSH_VERSION: &str = "tmp"; // env!("TSH_VERSION");
const PROXY: &str = "ztvsproxy03.zg.local:8080";
pub const TSH_BIN: &str = "teleport/teleporterdc_tsh.exe";
pub const TSH_LOCAL_CONFIG: &str = "teleporter.yaml";
const TSH_REMOTE_CONFIG: &str = "teleporter@bastion:storage/teleporter.config.yaml";

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
    res.arg(cmd);
    res
}

pub fn get_cfg(tsh_path: &str) -> Result<String> {
    let mut cmd = Command::new(tsh_path);
    cmd.args(["proxy", "app", "paolo-test", "--port", "47473"]);
    cmd.args(TSH_AUTH_ARGS);
    _ = dump!(&cmd);
    let mut ch = cmd.no_window().stdout(Stdio::piped()).spawn()?;

    let stdout = ch.stdout.take().expect("no stdout");

    let reader = BufReader::new(stdout);

    let mut result_value = None;

    for line in reader.lines() {
        let line = line.expect("failed to read line");
        println!("child said: {}", line);

        // Proxying connections to paolo-test on 127.0.0.1:61973
        use tauri::http::Response;
        if line.starts_with("Proxying connections to paolo-test") {
            let res = ureq::get("http://localhost:47473")
                .call()?
                .body_mut()
                .read_to_string()
                .context("call local uri")?;

            ch.kill()?;

            l!("get_cfg output: {:?}", res);
            result_value = Some(res);
        }
    }

    println!("{}", result_value.unwrap_or_default());

    ch.kill()?;

    Ok("".into())
}

pub fn execute_output(cmd: &mut Command) -> Result<String> {
    _ = dump!(&cmd);
    let out = cmd.no_window().output()?;
    if !out.status.success() {
        bail!("{}", dump!(String::from_utf8_lossy(&out.stderr)));
    }

    Ok(dump!(String::from_utf8_lossy(&out.stdout).into_owned()))
}

pub fn login(tsh_path: &str) -> Result<LoginResult> {
    let out = execute_output(tsh(tsh_path, "login").args(TSH_AUTH_ARGS))?;
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

// fn probe_google() {
//     let Ok(res) = Command::new("curl")
//         .args([
//             "-k",
//             "-s",
//             "-o",
//             "nul",
//             "-f",
//             "-LI",
//             "https://www.google.com",
//         ])
//         .no_window()
//         .status()
//     else {
//         l!("Error executing curl");
//         return;
//     };
//     if res.success() {
//         l!("Probing Google success");
//     } else {
//         l!("Probing Google failed with {:?}", res.code());
//     }
// }

// fn monitor_tunnels(state: &Arc<RwLock<State>>, tunnels: &Vec<Tunnel>) {
//     loop {
//         for t in tunnels.iter().filter(|x| x.enabled) {
//             if *state.read().unwrap() != State::Connected {
//                 continue;
//             };
//             thread::sleep(Duration::from_secs(60));
//             l!("Monitor {} {}", t.name, t.local);
//             if let Ok(_) = TcpStream::connect(f!("127.0.0.1:{}", t.local)) {
//                 l!("Monitor {}: success", t.name);
//             } else {
//                 l!("Monitor {}: fail", t.name);
//             }
//         }
//     }
// }

// fn monitor_probes(state: &Arc<RwLock<State>>, probes: &Vec<Tunnel>, interval_sec: u64) {
//     let interval_sec = if interval_sec == 0 { 20 } else { interval_sec };
//     let mut sleep = interval_sec;
//     let mut failures_count = 0;
//     loop {
//         for p in probes.iter().filter(|x| x.enabled) {
//             if *state.read().unwrap() != State::Connected {
//                 continue;
//             };
//             thread::sleep(Duration::from_secs(sleep));
//             l!("Probing {}...", p.local);
//             let proto = if p.secure { "https" } else { "http" };
//             let local = &p.local;
//             let Ok(res) = Command::new("curl")
//                 .args(["-k", "-s", "-o", "nul", "-f", "-LI", &f!("{proto}://127.0.0.1:{local}")])
//                 .no_window()
//                 .status()
//             else {
//                 continue;
//             };
//             if res.success() {
//                 failures_count = 0;
//                 sleep = interval_sec;
//                 l!("Probing success");
//             } else {
//                 failures_count += 1;
//                 sleep = 1;
//                 let code = res.code().unwrap_or_default();
//                 l!("Probing {proto} {local} fail {code}, failure {failures_count}");
//                 if failures_count >= 3 {
//                     probe_google();
//                     _ = kill_tsh().tap_err(|err| l!("Error restarting tsh {err:?}"));
//                 }
//             }
//         }
//     }
// }

// pub fn connect(opt: &Options, dispatch: &mut impl FnMut(Msg), state: &Arc<RwLock<State>>) {
//     let cfg = &opt.remote_config;
//     let enabled_tunnels = cfg.tunnels.clone().into_iter().filter(|x| x.enabled).collect_vec();
//     let enabled_probes = cfg.probes.clone().into_iter().filter(|x| x.enabled).collect_vec();
//     let tunnels = [enabled_tunnels, enabled_probes].concat();
//     let tunnels = tunnels.iter().map(|x| vec!["-L".to_string(), x.as_ssh()]).flatten().collect_vec();
//     let mut cmd = Command::new("cmd.exe");
//     cmd.arg("/c")
//         .arg(&f!(
//             "{} ssh -d -N {} {} 2>&1",
//             &opt.tsh_path.display(),
//             tunnels.join(" "),
//             &opt.config.bastion
//         ))
//         .no_window();
//     thread::scope(|scope| {
//         scope.spawn(|| monitor_probes(&state, &cfg.probes, cfg.probe_interval_sec));
//         scope.spawn(|| monitor_tunnels(&state, &cfg.tunnels));
//         let mut consecutive_fails = 0;
//         loop {
//             l!("while: {:?}", *state.read().unwrap());
//             let mut output = cmd.stdout(Stdio::piped()).no_window().spawn().expect("Cannot execute tsh");
//             let mut connected_sent = false;
//             if let Some(stdout) = output.stdout.take() {
//                 let mut stderr = BufReader::new(stdout);
//                 let mut buffer = [0; 1024];
//                 while let Ok(n_bytes) = stderr.read(&mut buffer[..]) {
//                     if n_bytes == 0 {
//                         break;
//                     }
//                     let res = String::from_utf8_lossy(&buffer);
//                     let res = res.replace("\n", "nnn");
//                     l!("[TSH] {res}");
//                     if !connected_sent && res.contains("Starting port forwarding") {
//                         consecutive_fails = 0;
//                         dispatch(Msg::UpdateState(Connected));
//                         connected_sent = true;
//                     }
//                 }
//             }
//             consecutive_fails += 1;
//             l!("Tsh exit, fail {consecutive_fails}");
//             if consecutive_fails > 2 {
//                 consecutive_fails = 0;
//                 l!("Trying to logout and login");
//                 dispatch(Msg::UpdateState(Relogin));
//                 _ = logout(opt).tap_err(|e| l!("Logout error: {e}"));
//                 _ = login(opt).tap_err(|e| l!("Login error: {e}"));
//             }
//             dispatch(Msg::UpdateState(Connecting));
//             std::thread::sleep(Duration::from_secs(1));
//         }
//     });
// }

fn filter_for_current_role(
    login_res: &LoginResult,
    mut config: teleporter_config::RemoteCfg,
) -> teleporter_config::RemoteCfg {
    let roles = &mut login_res.roles.split(",").map(|x| x.trim()).collect_vec();
    roles.push(&login_res.user);
    l!("{config:?}");
    config.tunnels = config
        .tunnels
        .into_iter()
        .filter(|x| {
            let xroles = &mut x.roles.split(",").map(|r| r.trim()).collect_vec();
            roles
                .iter_mut()
                .any(|r| xroles.iter_mut().any(|xr| r == xr))
        })
        .collect_vec();
    l!("role filtered: {config:?}");
    config
}

// pub fn save_to_local_config(opt: &Options) {
//     let yaml = serde_yaml::to_string(&opt.config).tap_err(|e| l!("{e}")).unwrap();
//     std::fs::write(&opt.config_path, yaml).tap_err(|e| l!("{e}")).unwrap();
// }

// pub fn get_config(opt: &Options, dispatch: &mut impl FnMut(Msg)) -> Result<()> {
//     dispatch(Msg::Progress("Login".into(), 100.));
//     let login_res = login(opt)?;
//     dispatch(Msg::Progress("Get config".into(), 100.));
//     execute(tsh(opt, "scp").args([TSH_REMOTE_CONFIG, "tmp.yaml"])).context("get config failed")?;
//     let config = std::fs::read_to_string("tmp.yaml")?;
//     let config: teleporter_config::RemoteCfg = serde_yaml::from_str(&config)?;
//     let config = filter_for_current_role(&login_res, config);
//     dispatch(Msg::Config(config));
//     Ok(())
// }

// pub fn init(opt: &Options, dispatch: &mut impl FnMut(Msg)) -> Result<()> {
//     check_tsh(opt, dispatch)?;
//     dispatch(Msg::Progress("Logout".into(), 100.));
//     logout(opt)?;
//     get_config(opt, dispatch)
// }

#[cfg(target_os = "windows")]
pub fn kill_tsh() -> Result<()> {
    Command::new("taskkill")
        .arg("/F")
        .arg("/IM")
        .arg("teleporterdc_tsh.exe")
        .no_window()
        .status()?;
    Command::new("taskkill")
        .arg("/F")
        .arg("/IM")
        .arg("teleporter_tsh.exe")
        .no_window()
        .status()?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn kill_tsh() -> Result<()> {
    Ok(())
}
