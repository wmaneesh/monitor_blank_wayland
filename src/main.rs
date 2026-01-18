mod wayland_layer;

use clap::Parser;
use std::path::PathBuf;
use std::{fs, process};
use wayland_layer::run_monitor_blank;

#[derive(Parser, Debug)]
#[command(name = "monitor_blank_wayland")]
#[command(about = "Blank selected monitors on Wayland")]
struct Args {
    /// Output names (e.g. DP-1 DP-2)
    outputs: Vec<String>,
}

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

    let args = Args::parse();

    if args.outputs.is_empty() {
        eprintln!("No outputs provided");
        return;
    }

    run_monitor_blank(args.outputs);

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
