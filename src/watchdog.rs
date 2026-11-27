//! Hang diagnostics. Each thread that must stay responsive (the dock, the dashboard, the desktop
//! widgets) writes a one-word "what I am doing now" before anything that could block. A tiny
//! watcher wakes every 2 s; when a thread has been in the same non-idle phase for more than 5 s it
//! appends one line to `%APPDATA%\Vire\hang.log` naming the thread and the phase — so a
//! "Vire.exe is not responding" can be traced to the exact call instead of guessed at.
//! Cost: one uncontended mutex write per phase change and a 2-second sleeping thread.

use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const DOCK: usize = 0;
pub const UI: usize = 1;
pub const WIDGETS: usize = 2;
const NAMES: [&str; 3] = ["dock", "dashboard", "widgets"];

static PHASE: [Mutex<(&'static str, Option<Instant>)>; 3] = [Mutex::new(("idle", None)), Mutex::new(("idle", None)), Mutex::new(("idle", None))];

/// "I am now in `name`" (`"idle"` = waiting for messages, which is fine for any length of time).
pub fn phase(thread: usize, name: &'static str) {
    if let Ok(mut p) = PHASE[thread].lock() { *p = (name, (name != "idle").then(Instant::now)); }
}

pub fn start() {
    let _ = std::thread::Builder::new().name("vire-watchdog".into()).spawn(|| {
        let mut reported: [Option<Instant>; 3] = [None; 3];
        loop {
            std::thread::sleep(Duration::from_secs(2));
            for t in 0..3 {
                let (name, since) = match PHASE[t].lock() { Ok(p) => *p, Err(_) => continue };
                let Some(since) = since else { reported[t] = None; continue };
                if since.elapsed() > Duration::from_secs(5) && reported[t] != Some(since) {
                    reported[t] = Some(since);
                    log(&format!("{} thread stuck for {} s in phase '{}'", NAMES[t], since.elapsed().as_secs(), name));
                }
            }
        }
    });
}

fn log(line: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(crate::model::data_dir().join("hang.log")) {
        let _ = writeln!(f, "{} {line}", crate::model::now_secs());
    }
}
