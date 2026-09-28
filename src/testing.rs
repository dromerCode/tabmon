//! Test doubles shared by the unit tests.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::time::Duration;

use anyhow::Result;

use crate::error::UserError;
use crate::system::{Output, System};

/// PID that `FakeSystem::spawn_detached` hands out.
pub const SPAWN_PID: u32 = 4242;

pub fn ok(stdout: &str) -> Output {
    Output {
        success: true,
        stdout: stdout.to_string(),
        stderr: String::new(),
    }
}

pub fn fail(stderr: &str) -> Output {
    Output {
        success: false,
        stdout: String::new(),
        stderr: stderr.to_string(),
    }
}

/// Records every call and answers with scripted responses.
///
/// Responses for a command are consumed in order; the last one keeps being returned.
/// Commands without a scripted response succeed with empty output.
#[derive(Default)]
pub struct FakeSystem {
    calls: RefCell<Vec<String>>,
    responses: RefCell<HashMap<String, VecDeque<Result<Output, UserError>>>>,
    processes: RefCell<HashMap<u32, String>>,
    spawn_dies: RefCell<bool>,
    modules: RefCell<HashSet<String>>,
}

impl FakeSystem {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn respond(&self, cmd: &str, out: Output) {
        self.push(cmd, Ok(out));
    }

    pub fn respond_err(&self, cmd: &str, err: UserError) {
        self.push(cmd, Err(err));
    }

    pub fn add_process(&self, pid: u32, name: &str) {
        self.processes.borrow_mut().insert(pid, name.to_string());
    }

    /// When set, spawned programs exit immediately.
    pub fn set_spawn_dies(&self, dies: bool) {
        *self.spawn_dies.borrow_mut() = dies;
    }

    pub fn load_module(&self, name: &str) {
        self.modules.borrow_mut().insert(name.to_string());
    }

    pub fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }

    fn push(&self, cmd: &str, response: Result<Output, UserError>) {
        self.responses
            .borrow_mut()
            .entry(cmd.to_string())
            .or_default()
            .push_back(response);
    }
}

fn key(program: &str, args: &[&str]) -> String {
    std::iter::once(program)
        .chain(args.iter().copied())
        .collect::<Vec<_>>()
        .join(" ")
}

impl System for FakeSystem {
    fn run(&self, program: &str, args: &[&str]) -> Result<Output> {
        let key = key(program, args);
        self.calls.borrow_mut().push(key.clone());
        let mut responses = self.responses.borrow_mut();
        let response = match responses.get_mut(&key) {
            Some(queue) if queue.len() > 1 => queue.pop_front().unwrap(),
            Some(queue) => queue.front().unwrap().clone(),
            None => Ok(ok("")),
        };
        response.map_err(Into::into)
    }

    fn spawn_detached(&self, program: &str, args: &[&str], _log: &Path) -> Result<u32> {
        self.calls
            .borrow_mut()
            .push(format!("spawn {}", key(program, args)));
        if !*self.spawn_dies.borrow() {
            self.add_process(SPAWN_PID, program);
        }
        Ok(SPAWN_PID)
    }

    fn process_name(&self, pid: u32) -> Option<String> {
        self.processes.borrow().get(&pid).cloned()
    }

    fn terminate(&self, pid: u32) -> Result<()> {
        self.calls.borrow_mut().push(format!("terminate {pid}"));
        self.processes.borrow_mut().remove(&pid);
        Ok(())
    }

    fn module_loaded(&self, module: &str) -> bool {
        self.modules.borrow().contains(module)
    }

    fn sleep(&self, _duration: Duration) {}
}
