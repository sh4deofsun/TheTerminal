//! Single TOML config file (decision 14). No settings GUI in v1.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Config {
    pub font_family: String,
    pub font_size: u16,
    pub scrollback: usize,
    pub autocomplete: bool,
    pub hook: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            font_family: "Menlo, Monaco, monospace".into(),
            font_size: 13,
            scrollback: 10_000,
            autocomplete: true,
            hook: true,
        }
    }
}

pub fn load() -> Config {
    let Some(dir) = dirs::config_dir() else {
        return Config::default();
    };
    let path = dir.join("theterminal").join("config.toml");
    match std::fs::read_to_string(&path) {
        Ok(s) => toml::from_str(&s).unwrap_or_default(),
        Err(_) => {
            let cfg = Config::default();
            let _ = std::fs::create_dir_all(path.parent().unwrap());
            let _ = std::fs::write(&path, toml::to_string_pretty(&cfg).unwrap_or_default());
            cfg
        }
    }
}
