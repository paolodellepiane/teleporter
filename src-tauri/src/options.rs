use crate::{
    teleporter_config,
    tsh::{TSH_BIN, TSH_LOCAL_CONFIG},
};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Options {
    pub config_dir: PathBuf,
    pub config_path: PathBuf,
    pub log_dir: PathBuf,
    pub tsh_path: PathBuf,
    pub config: teleporter_config::Cfg,
    pub remote_config: teleporter_config::RemoteCfg,
}

impl Options {
    pub fn new() -> Options {
        let user_dirs = directories::UserDirs::new().expect("can't get user dirs");
        let config_dir = user_dirs.home_dir().join(".config/teleporterdc");
        let config_path = config_dir.join(TSH_LOCAL_CONFIG);
        let log_dir = config_dir.join("log");
        let tsh_path = config_dir.join(TSH_BIN);
        let config = if config_path.exists() {
            if let Ok(config) = std::fs::read_to_string(&config_path) {
                if let Ok(config) = serde_json::from_str(&config) {
                    config
                } else {
                    teleporter_config::Cfg::default()
                }
            } else {
                teleporter_config::Cfg::default()
            }
        } else {
            teleporter_config::Cfg::default()
        };

        Options {
            config_dir,
            config_path,
            log_dir,
            tsh_path,
            config,
            remote_config: Default::default(),
        }
    }
}
