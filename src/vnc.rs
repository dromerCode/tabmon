use std::path::Path;
use std::time::Duration;

use anyhow::Result;

use crate::error::UserError;
use crate::system::System;

/// Starts wayvnc on localhost for one output and checks that it stays up.
pub fn start(sys: &dyn System, output: &str, port: u16, max_fps: u32, log: &Path) -> Result<u32> {
    let fps = format!("--max-fps={max_fps}");
    let port_arg = port.to_string();
    let pid = sys.spawn_detached(
        "wayvnc",
        &["-r", &fps, "-o", output, "127.0.0.1", &port_arg],
        log,
    )?;
    sys.sleep(Duration::from_millis(500));
    if !is_running(sys, pid) {
        return Err(UserError::new("wayvnc exited right after starting")
            .hint(format!(
                "see {}; is port {port} already in use?",
                log.display()
            ))
            .into());
    }
    Ok(pid)
}

/// True only if `pid` is alive and still wayvnc (PIDs get reused).
pub fn is_running(sys: &dyn System, pid: u32) -> bool {
    sys.process_name(pid).as_deref() == Some("wayvnc")
}

pub fn stop(sys: &dyn System, pid: u32) -> Result<()> {
    if is_running(sys, pid) {
        sys.terminate(pid)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::render;
    use crate::testing::{FakeSystem, SPAWN_PID};

    #[test]
    fn start_launches_wayvnc_on_localhost() {
        let fake = FakeSystem::new();
        let pid = start(&fake, "Virtual-1", 5900, 90, Path::new("/tmp/log")).unwrap();
        assert_eq!(pid, SPAWN_PID);
        assert_eq!(
            fake.calls(),
            vec!["spawn wayvnc -r --max-fps=90 -o Virtual-1 127.0.0.1 5900"]
        );
    }

    #[test]
    fn start_fails_if_wayvnc_exits() {
        let fake = FakeSystem::new();
        fake.set_spawn_dies(true);
        let err = start(
            &fake,
            "Virtual-1",
            5900,
            90,
            Path::new("/run/tabmon/wayvnc.log"),
        )
        .unwrap_err();
        assert_eq!(
            render(&err),
            "error: wayvnc exited right after starting\nhint: see /run/tabmon/wayvnc.log; is port 5900 already in use?"
        );
    }

    #[test]
    fn is_running_checks_the_process_name() {
        let fake = FakeSystem::new();
        fake.add_process(10, "wayvnc");
        fake.add_process(11, "firefox");
        assert!(is_running(&fake, 10));
        assert!(!is_running(&fake, 11));
        assert!(!is_running(&fake, 12));
    }

    #[test]
    fn stop_only_kills_wayvnc() {
        let fake = FakeSystem::new();
        fake.add_process(10, "wayvnc");
        fake.add_process(11, "firefox");
        stop(&fake, 10).unwrap();
        stop(&fake, 11).unwrap();
        stop(&fake, 12).unwrap();
        assert_eq!(fake.calls(), vec!["terminate 10"]);
    }
}
