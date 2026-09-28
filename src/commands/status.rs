use anyhow::Result;

use super::Ctx;
use crate::state;
use crate::vnc;

pub fn run(ctx: &Ctx) -> Result<String> {
    Ok(match state::load(&ctx.paths)? {
        Some(s) if vnc::is_running(ctx.sys, s.pid) => format!(
            "on\n  output: {}\n  port: {} (localhost only)\n  device: {}\n  wayvnc pid: {}",
            s.output, s.port, s.serial, s.pid
        ),
        Some(_) => "off (stale session left behind; `tabmon off` cleans it up)".into(),
        None => "off".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::tests_support::ctx;
    use crate::compositor::niri::Niri;
    use crate::state::State;
    use crate::testing::{FakeSystem, temp_paths};

    fn saved() -> State {
        State {
            pid: 77,
            port: 5900,
            serial: "ABC123".into(),
            output: "Virtual-1".into(),
            created_reverse: true,
        }
    }

    #[test]
    fn reports_off() {
        let (_dir, paths) = temp_paths();
        let fake = FakeSystem::new();
        let niri = Niri::new(&fake);
        assert_eq!(run(&ctx(&fake, &niri, &paths)).unwrap(), "off");
    }

    #[test]
    fn reports_on_with_details() {
        let (_dir, paths) = temp_paths();
        let fake = FakeSystem::new();
        state::save(&paths, &saved()).unwrap();
        fake.add_process(77, "wayvnc");
        let niri = Niri::new(&fake);
        assert_eq!(
            run(&ctx(&fake, &niri, &paths)).unwrap(),
            "on\n  output: Virtual-1\n  port: 5900 (localhost only)\n  device: ABC123\n  wayvnc pid: 77"
        );
    }

    #[test]
    fn reports_stale_state() {
        let (_dir, paths) = temp_paths();
        let fake = FakeSystem::new();
        state::save(&paths, &saved()).unwrap();
        let niri = Niri::new(&fake);
        assert_eq!(
            run(&ctx(&fake, &niri, &paths)).unwrap(),
            "off (stale session left behind; `tabmon off` cleans it up)"
        );
    }
}
