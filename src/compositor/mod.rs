pub mod hyprland;
pub mod niri;

use std::ffi::OsString;

use anyhow::Result;

use crate::error::UserError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputState {
    Missing,
    Off,
    On,
}

/// The compositors tabmon supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Niri,
    Hyprland,
}

impl Kind {
    pub fn from_env() -> Result<Kind> {
        Self::from_vars(
            std::env::var_os("NIRI_SOCKET"),
            std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE"),
        )
    }

    /// Picks the compositor from the variables each one sets in its session.
    pub fn from_vars(
        niri_socket: Option<OsString>,
        hyprland_signature: Option<OsString>,
    ) -> Result<Kind> {
        let set = |v: &Option<OsString>| v.as_ref().is_some_and(|v| !v.is_empty());
        match (set(&niri_socket), set(&hyprland_signature)) {
            (true, false) => Ok(Kind::Niri),
            (false, true) => Ok(Kind::Hyprland),
            (false, false) => Err(UserError::new(
                "no supported compositor found (niri or Hyprland)",
            )
            .hint("run tabmon from inside your niri or Hyprland session")
            .into()),
            (true, true) => Err(UserError::new(
                "both NIRI_SOCKET and HYPRLAND_INSTANCE_SIGNATURE are set",
            )
            .hint("unset the one of the compositor you are not using")
            .into()),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Niri => "niri",
            Kind::Hyprland => "Hyprland",
        }
    }

    /// Output used when the config does not name one.
    pub fn default_output(self) -> &'static str {
        match self {
            Kind::Niri => "Virtual-1",
            Kind::Hyprland => "TABMON",
        }
    }
}

/// What tabmon needs from a Wayland compositor.
pub trait Compositor {
    fn kind(&self) -> Kind;
    fn output_state(&self, output: &str) -> Result<OutputState>;
    fn set_output(&self, output: &str, on: bool) -> Result<()>;
    /// What to tell the user when `output_state` says `Missing`.
    fn missing_output(&self, output: &str) -> UserError;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::render;

    #[test]
    fn detects_niri() {
        let kind = Kind::from_vars(Some("/run/user/1000/niri.sock".into()), None).unwrap();
        assert_eq!(kind, Kind::Niri);
        assert_eq!(kind.name(), "niri");
        assert_eq!(kind.default_output(), "Virtual-1");
    }

    #[test]
    fn detects_hyprland() {
        let kind = Kind::from_vars(None, Some("abc_123".into())).unwrap();
        assert_eq!(kind, Kind::Hyprland);
        assert_eq!(kind.name(), "Hyprland");
        assert_eq!(kind.default_output(), "TABMON");
    }

    #[test]
    fn empty_variables_do_not_count() {
        let kind = Kind::from_vars(Some("".into()), Some("abc_123".into())).unwrap();
        assert_eq!(kind, Kind::Hyprland);
    }

    #[test]
    fn no_compositor_is_an_error() {
        let err = Kind::from_vars(None, None).unwrap_err();
        assert_eq!(
            render(&err),
            "error: no supported compositor found (niri or Hyprland)\nhint: run tabmon from inside your niri or Hyprland session"
        );
    }

    #[test]
    fn both_compositors_is_an_error() {
        let err = Kind::from_vars(Some("/run/niri.sock".into()), Some("abc".into())).unwrap_err();
        assert_eq!(
            render(&err),
            "error: both NIRI_SOCKET and HYPRLAND_INSTANCE_SIGNATURE are set\nhint: unset the one of the compositor you are not using"
        );
    }
}
