mod wayland_layer;

use crate::wayland_layer::run_monitor_blank;
use std::path::PathBuf;
use std::{fs, process};

fn main() {
    if try_toggle_existing_instance() {
        // Existing instance told to exit → we're done
        return;
    }

    create_lockfile();

    // Ensure cleanup
    ctrlc::set_handler(|| {
        cleanup_lockfile();
        std::process::exit(0);
    })
    .unwrap();

    run_monitor_blank();

    cleanup_lockfile();
}

fn lockfile_path() -> PathBuf {
    let runtime = std::env::var("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR not set");
    PathBuf::from(runtime).join("monitor_blank.lock")
}

fn try_toggle_existing_instance() -> bool {
    let path = lockfile_path();

    if let Ok(pid_str) = fs::read_to_string(&path) {
        if let Ok(pid) = pid_str.trim().parse::<i32>() {
            if unsafe { libc::kill(pid, 0) } == 0 {
                unsafe {
                    libc::kill(pid, libc::SIGTERM);
                }
                return true;
            }
        }
        let _ = fs::remove_file(&path);
    }
    false
}

fn create_lockfile() {
    let path = lockfile_path();
    let pid = process::id().to_string();
    fs::write(path, pid).expect("Failed to write lockfile");
}

fn cleanup_lockfile() {
    let _ = std::fs::remove_file(lockfile_path());
}
