pub mod off;
pub mod on;
pub mod status;
pub mod toggle;

use anyhow::Result;

use crate::compositor::Compositor;
use crate::config::{Config, Paths};
use crate::state::{self, State};
use crate::system::System;
use crate::vnc;

/// Everything a command needs.
pub struct Ctx<'a> {
    pub sys: &'a dyn System,
    pub compositor: &'a dyn Compositor,
    pub config: Config,
    pub paths: Paths,
}

/// State of a previous `on` whose wayvnc is still running.
pub fn active_state(ctx: &Ctx) -> Result<Option<State>> {
    Ok(state::load(&ctx.paths)?.filter(|s| vnc::is_running(ctx.sys, s.pid)))
}

#[cfg(test)]
pub mod tests_support {
    use super::Ctx;
    use crate::compositor::Compositor;
    use crate::config::{Config, Paths};
    use crate::system::System;

    pub const ON: &str = include_str!("../../tests/fixtures/niri-outputs-on.json");
    pub const OFF: &str = include_str!("../../tests/fixtures/niri-outputs-off.json");
    pub const MISSING: &str = include_str!("../../tests/fixtures/niri-outputs-missing.json");
    pub const ADB_ONE: &str = "List of devices attached\nABC123\tdevice\n\n";

    pub fn ctx<'a>(sys: &'a dyn System, compositor: &'a dyn Compositor, paths: &Paths) -> Ctx<'a> {
        Ctx {
            sys,
            compositor,
            config: Config::default(),
            paths: paths.clone(),
        }
    }
}
