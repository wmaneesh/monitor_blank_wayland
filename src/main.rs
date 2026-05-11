mod wayland_layer;
use clap::Parser;
use signal_hook::consts::signal::*;
use signal_hook::flag;
use std::path::PathBuf;
use std::sync::{Arc, atomic::AtomicBool};
use std::{fs, process, process::Command};
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

    let args = Args::parse();

    save_monitor_config(&args.outputs);

    let term = Arc::new(AtomicBool::new(false));

    flag::register(SIGTERM, Arc::clone(&term)).unwrap();
    flag::register(SIGINT, Arc::clone(&term)).unwrap();

    if args.outputs.is_empty() {
        eprintln!("No outputs provided");
        return;
    }

    run_monitor_blank(args.outputs, term);

    cleanup_lockfile();
    cleanup_configfile();
}

fn lockfile_path() -> PathBuf {
    let runtime = std::env::var("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR not set");
    PathBuf::from(runtime).join("monitor_blank.lock")
}

fn configfile_path() -> PathBuf {
    let runtime = std::env::var("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR not set");
    PathBuf::from(runtime).join("monitor_blank.conf")
}

fn try_toggle_existing_instance() -> bool {
    let path = lockfile_path();

    if let Ok(pid_str) = fs::read_to_string(&path) {
        if let Ok(pid) = pid_str.trim().parse::<i32>() {
            if unsafe { libc::kill(pid, 0) } == 0 {
                restore_brightness_from_config();

                unsafe {
                    libc::kill(pid, libc::SIGTERM);
                }

                cleanup_configfile();

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

fn save_monitor_config(outputs: &[String]) {
    let path = configfile_path();
    let content = outputs.join("\n");
    fs::write(path, content).expect("Failed to write config");
}

fn cleanup_lockfile() {
    let _ = std::fs::remove_file(lockfile_path());
}

fn cleanup_configfile() {
    let _ = std::fs::remove_file(configfile_path());
}

fn restore_brightness_from_config() {
    let path = configfile_path();

    let Ok(content) = fs::read_to_string(path) else {
        return;
    };

    for line in content.lines() {
        let parts: Vec<&str> = line.split(':').collect();

        if parts.len() != 3 {
            continue;
        }

        let output = parts[0];
        let restore = parts[2];

        let _ = Command::new("ddcutil")
            .args(["--bus", output, "setvcp", "10", restore])
            .spawn();
    }
}
