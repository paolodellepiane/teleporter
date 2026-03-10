use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cfg {
    pub bastion: String,
    pub connect_at_start: bool,
    pub start_minimized: bool,
    pub position: Option<(i32, i32)>,
}

impl Default for Cfg {
    fn default() -> Self {
        Cfg {
            bastion: "teleporter@bastion".into(),
            connect_at_start: true,
            start_minimized: false,
            position: None,
        }
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteCfg {
    pub probes: Vec<Tunnel>,
    pub tunnels: Vec<Tunnel>,
    pub probe_interval_sec: u64,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tunnel {
    pub local: String,
    pub name: String,
    pub group: String,
    pub dest: String,
    pub remote: String,
    pub enabled: bool,
    #[serde(default)]
    pub secure: bool,
    pub roles: String,
    pub bastion: Option<String>,
}

impl Tunnel {
    pub fn as_ssh(&self) -> String {
        format!("{}:{}:{}", self.local, self.dest, self.remote)
    }
}
