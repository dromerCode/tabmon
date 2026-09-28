use std::fs::{File, OpenOptions};

use anyhow::{Context, Result};
use nix::errno::Errno;
use nix::fcntl::{Flock, FlockArg};

use crate::config::Paths;
use crate::error::UserError;

/// Held while a command that changes the session runs; released when dropped or when the
/// process exits, even if it crashes.
pub struct Lock {
    _file: Flock<File>,
}

/// Takes the lock, or returns `None` if another tabmon command holds it.
pub fn try_acquire(paths: &Paths) -> Result<Option<Lock>> {
    std::fs::create_dir_all(&paths.runtime_dir)
        .with_context(|| format!("could not create {}", paths.runtime_dir.display()))?;
    let path = paths.runtime_dir.join("lock");
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .with_context(|| format!("could not open {}", path.display()))?;
    match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
        Ok(file) => Ok(Some(Lock { _file: file })),
        Err((_, Errno::EWOULDBLOCK)) => Ok(None),
        Err((_, errno)) => {
            Err(anyhow::Error::new(errno).context(format!("could not lock {}", path.display())))
        }
    }
}

pub fn busy() -> UserError {
    UserError::new("another tabmon command is still running").hint("wait a moment and try again")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::render;
    use crate::testing::temp_paths;

    #[test]
    fn first_lock_is_granted_and_creates_the_runtime_dir() {
        let (_dir, paths) = temp_paths();
        assert!(try_acquire(&paths).unwrap().is_some());
        assert!(paths.runtime_dir.join("lock").exists());
    }

    #[test]
    fn second_lock_is_refused_while_the_first_is_held() {
        let (_dir, paths) = temp_paths();
        let _first = try_acquire(&paths).unwrap().unwrap();
        assert!(try_acquire(&paths).unwrap().is_none());
    }

    #[test]
    fn lock_can_be_taken_again_after_release() {
        let (_dir, paths) = temp_paths();
        drop(try_acquire(&paths).unwrap().unwrap());
        assert!(try_acquire(&paths).unwrap().is_some());
    }

    #[test]
    fn busy_error_explains_what_to_do() {
        assert_eq!(
            render(&busy().into()),
            "error: another tabmon command is still running\nhint: wait a moment and try again"
        );
    }
}
