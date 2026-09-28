use std::env;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Deserialize;

use crate::error::UserError;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// vkms output defined in niri's config.kdl.
    pub output: String,
    pub port: u16,
    pub max_fps: u32,
    /// Desktop notifications on on/off and on errors.
    pub notify: bool,
    /// Only needed when several adb devices are connected.
    pub adb_serial: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            output: "Virtual-1".into(),
            port: 5900,
            max_fps: 90,
            notify: true,
            adb_serial: None,
        }
    }
}

impl Config {
    /// Reads the config file; a missing file means all defaults.
    pub fn load(path: &Path) -> Result<Config> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Config::default()),
            Err(e) => {
                return Err(
                    anyhow::Error::new(e).context(format!("could not read {}", path.display()))
                );
            }
        };
        toml::from_str(&text).map_err(|e| {
            UserError::new(format!("invalid config file {}: {e}", path.display()))
                .hint("see the configuration section of the README")
                .into()
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub config_file: PathBuf,
    /// `$XDG_RUNTIME_DIR/tabmon`.
    pub runtime_dir: PathBuf,
}

impl Paths {
    pub fn from_env() -> Result<Paths> {
        Self::from_vars(
            env::var_os("XDG_CONFIG_HOME"),
            env::var_os("HOME"),
            env::var_os("XDG_RUNTIME_DIR"),
        )
    }

    pub fn from_vars(
        xdg_config_home: Option<OsString>,
        home: Option<OsString>,
        xdg_runtime_dir: Option<OsString>,
    ) -> Result<Paths> {
        let non_empty = |v: &OsString| !v.is_empty();
        let config_home = match xdg_config_home.filter(non_empty) {
            Some(dir) => PathBuf::from(dir),
            None => {
                let home = home
                    .filter(non_empty)
                    .ok_or_else(|| UserError::new("neither XDG_CONFIG_HOME nor HOME is set"))?;
                PathBuf::from(home).join(".config")
            }
        };
        let runtime = xdg_runtime_dir.filter(non_empty).ok_or_else(|| {
            UserError::new("XDG_RUNTIME_DIR is not set")
                .hint("run tabmon from inside your desktop session")
        })?;
        Ok(Paths {
            config_file: config_home.join("tabmon").join("config.toml"),
            runtime_dir: PathBuf::from(runtime).join("tabmon"),
        })
    }

    pub fn state_file(&self) -> PathBuf {
        self.runtime_dir.join("state")
    }

    pub fn vnc_log(&self) -> PathBuf {
        self.runtime_dir.join("wayvnc.log")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::render;
    use crate::testing::temp_paths;

    fn write_config(text: &str) -> (tempfile::TempDir, Paths) {
        let (dir, paths) = temp_paths();
        std::fs::write(&paths.config_file, text).unwrap();
        (dir, paths)
    }

    #[test]
    fn missing_file_gives_defaults() {
        let (_dir, paths) = temp_paths();
        let config = Config::load(&paths.config_file).unwrap();
        assert_eq!(
            config,
            Config {
                output: "Virtual-1".into(),
                port: 5900,
                max_fps: 90,
                notify: true,
                adb_serial: None
            }
        );
    }

    #[test]
    fn partial_file_is_completed_with_defaults() {
        let (_dir, paths) = write_config("port = 5901\nadb_serial = \"ABC123\"\n");
        let config = Config::load(&paths.config_file).unwrap();
        assert_eq!(config.port, 5901);
        assert_eq!(config.adb_serial.as_deref(), Some("ABC123"));
        assert_eq!(config.output, "Virtual-1");
        assert!(config.notify);
    }

    #[test]
    fn invalid_toml_is_a_clear_error() {
        let (_dir, paths) = write_config("port = = 3");
        let text = render(&Config::load(&paths.config_file).unwrap_err());
        assert!(text.starts_with("error: invalid config file"), "{text}");
        assert!(text.contains("config.toml"), "{text}");
    }

    #[test]
    fn unknown_key_is_rejected() {
        let (_dir, paths) = write_config("max-fps = 60\n");
        let text = render(&Config::load(&paths.config_file).unwrap_err());
        assert!(text.contains("unknown field"), "{text}");
        assert!(text.contains("max-fps"), "{text}");
    }

    #[test]
    fn paths_use_xdg_config_home() {
        let paths = Paths::from_vars(
            Some("/cfg".into()),
            Some("/home/u".into()),
            Some("/run/user/1000".into()),
        )
        .unwrap();
        assert_eq!(paths.config_file, PathBuf::from("/cfg/tabmon/config.toml"));
        assert_eq!(paths.runtime_dir, PathBuf::from("/run/user/1000/tabmon"));
        assert_eq!(
            paths.state_file(),
            PathBuf::from("/run/user/1000/tabmon/state")
        );
        assert_eq!(
            paths.vnc_log(),
            PathBuf::from("/run/user/1000/tabmon/wayvnc.log")
        );
    }

    #[test]
    fn paths_fall_back_to_home_config() {
        let paths = Paths::from_vars(
            Some("".into()),
            Some("/home/u".into()),
            Some("/run/user/1000".into()),
        )
        .unwrap();
        assert_eq!(
            paths.config_file,
            PathBuf::from("/home/u/.config/tabmon/config.toml")
        );
    }

    #[test]
    fn missing_runtime_dir_is_an_error() {
        let err = Paths::from_vars(None, Some("/home/u".into()), None).unwrap_err();
        assert_eq!(
            render(&err),
            "error: XDG_RUNTIME_DIR is not set\nhint: run tabmon from inside your desktop session"
        );
    }
}
