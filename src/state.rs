use std::io;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::Paths;

/// What `tabmon on` started, so `off` undoes exactly that.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub pid: u32,
    pub port: u16,
    pub serial: String,
    pub output: String,
}

/// Saved state, or `None` if there is none or it cannot be understood.
pub fn load(paths: &Paths) -> Result<Option<State>> {
    let path = paths.state_file();
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok(toml::from_str(&text).ok()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(anyhow::Error::new(e).context(format!("could not read {}", path.display()))),
    }
}

pub fn save(paths: &Paths, state: &State) -> Result<()> {
    std::fs::create_dir_all(&paths.runtime_dir)
        .with_context(|| format!("could not create {}", paths.runtime_dir.display()))?;
    let path = paths.state_file();
    std::fs::write(&path, toml::to_string(state)?)
        .with_context(|| format!("could not write {}", path.display()))
}

pub fn clear(paths: &Paths) -> Result<()> {
    let path = paths.state_file();
    match std::fs::remove_file(&path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => {
            Err(anyhow::Error::new(e).context(format!("could not remove {}", path.display())))
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::temp_paths;

    fn sample() -> State {
        State {
            pid: 4242,
            port: 5900,
            serial: "ABC123".into(),
            output: "Virtual-1".into(),
        }
    }

    #[test]
    fn save_then_load_round_trips() {
        let (_dir, paths) = temp_paths();
        save(&paths, &sample()).unwrap();
        assert_eq!(load(&paths).unwrap(), Some(sample()));
    }

    #[test]
    fn missing_state_is_none() {
        let (_dir, paths) = temp_paths();
        assert_eq!(load(&paths).unwrap(), None);
    }

    #[test]
    fn corrupt_state_is_none() {
        let (_dir, paths) = temp_paths();
        std::fs::create_dir_all(&paths.runtime_dir).unwrap();
        std::fs::write(paths.state_file(), "pid = \"not a number\"").unwrap();
        assert_eq!(load(&paths).unwrap(), None);
    }

    #[test]
    fn clear_removes_state_and_tolerates_absence() {
        let (_dir, paths) = temp_paths();
        save(&paths, &sample()).unwrap();
        clear(&paths).unwrap();
        assert_eq!(load(&paths).unwrap(), None);
        clear(&paths).unwrap();
    }
}
