// This file is part of the uutils util-linux package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use clap::{crate_version, Arg, ArgAction, ArgGroup, Command};
use std::ffi::OsString;
use std::io;
use std::os::unix::process::CommandExt;
use std::process;
use uucore::error::{UResult, USimpleError};
use uucore::{format_usage, help_about, help_usage};

const ABOUT: &str = help_about!("chrt.md");
const USAGE: &str = help_usage!("chrt.md");

fn policy_name(policy: u32) -> &'static str {
    match policy as i32 {
        libc::SCHED_OTHER => "SCHED_OTHER",
        libc::SCHED_FIFO => "SCHED_FIFO",
        libc::SCHED_RR => "SCHED_RR",
        libc::SCHED_BATCH => "SCHED_BATCH",
        libc::SCHED_IDLE => "SCHED_IDLE",
        libc::SCHED_DEADLINE => "SCHED_DEADLINE",
        _ => "unknown",
    }
}

/// Wrapper for sched_getattr syscall.
fn sched_getattr(pid: i32) -> io::Result<libc::sched_attr> {
    let mut attr: libc::sched_attr = unsafe { std::mem::zeroed() };
    attr.size = std::mem::size_of::<libc::sched_attr>() as u32;

    let ret = unsafe {
        libc::syscall(
            libc::SYS_sched_getattr,
            pid,
            &mut attr as *mut libc::sched_attr,
            attr.size,
            0u32,
        )
    };
    if ret < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(attr)
    }
}

/// Wrapper for sched_setattr syscall.
fn sched_setattr(pid: i32, attr: &libc::sched_attr) -> io::Result<()> {
    let ret = unsafe {
        libc::syscall(
            libc::SYS_sched_setattr,
            pid,
            attr as *const libc::sched_attr,
            0u32,
        )
    };
    if ret < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Get all thread IDs for a given PID by reading /proc/<pid>/task/
fn get_tids(pid: i32) -> Vec<i32> {
    let task_dir = format!("/proc/{pid}/task");
    let Ok(entries) = std::fs::read_dir(&task_dir) else {
        return vec![pid];
    };

    entries
        .flatten()
        .filter_map(|e| e.file_name().to_string_lossy().parse::<i32>().ok())
        .collect()
}

fn print_sched_info(pid: i32) -> UResult<()> {
    let attr = sched_getattr(pid)
        .map_err(|e| USimpleError::new(1, format!("failed to get pid {pid}'s policy: {e}")))?;

    let policy = attr.sched_policy & !(libc::SCHED_RESET_ON_FORK as u32);
    println!(
        "pid {pid}'s current scheduling policy: {}",
        policy_name(policy)
    );
    println!(
        "pid {pid}'s current scheduling priority: {}",
        attr.sched_priority
    );

    // Show runtime for deadline and other policies that use it
    if attr.sched_runtime > 0 {
        println!(
            "pid {pid}'s current runtime parameter: {}",
            attr.sched_runtime
        );
    }

    Ok(())
}

fn print_max_priorities() {
    let policies = [
        ("SCHED_OTHER", libc::SCHED_OTHER),
        ("SCHED_FIFO", libc::SCHED_FIFO),
        ("SCHED_RR", libc::SCHED_RR),
        ("SCHED_BATCH", libc::SCHED_BATCH),
        ("SCHED_IDLE", libc::SCHED_IDLE),
        ("SCHED_DEADLINE", libc::SCHED_DEADLINE),
    ];

    for (name, policy) in policies {
        let min = unsafe { libc::sched_get_priority_min(policy) };
        let max = unsafe { libc::sched_get_priority_max(policy) };
        println!("{name} min/max priority\t: {min}/{max}");
    }
}

#[uucore::main]
pub fn uumain(args: impl uucore::Args) -> UResult<()> {
    let matches = uu_app().try_get_matches_from(args)?;

    // -m: show min/max priorities and exit
    if matches.get_flag("max") {
        print_max_priorities();
        return Ok(());
    }

    let pid_mode = matches.get_flag("pid");
    let all_tasks = matches.get_flag("all-tasks");
    let reset_on_fork = matches.get_flag("reset-on-fork");
    let verbose = matches.get_flag("verbose");

    // Determine the policy from flags
    let policy: Option<i32> = if matches.get_flag("batch") {
        Some(libc::SCHED_BATCH)
    } else if matches.get_flag("deadline") {
        Some(libc::SCHED_DEADLINE)
    } else if matches.get_flag("fifo") {
        Some(libc::SCHED_FIFO)
    } else if matches.get_flag("idle") {
        Some(libc::SCHED_IDLE)
    } else if matches.get_flag("other") {
        Some(libc::SCHED_OTHER)
    } else if matches.get_flag("rr") {
        Some(libc::SCHED_RR)
    } else {
        None
    };

    // DEADLINE params
    let sched_runtime = matches
        .get_one::<String>("sched-runtime")
        .map(|s| s.parse::<u64>())
        .transpose()
        .map_err(|_| USimpleError::new(1, "invalid runtime parameter"))?;
    let sched_period = matches
        .get_one::<String>("sched-period")
        .map(|s| s.parse::<u64>())
        .transpose()
        .map_err(|_| USimpleError::new(1, "invalid period parameter"))?;
    let sched_deadline = matches
        .get_one::<String>("sched-deadline")
        .map(|s| s.parse::<u64>())
        .transpose()
        .map_err(|_| USimpleError::new(1, "invalid deadline parameter"))?;

    let positional: Vec<OsString> = matches
        .get_many::<OsString>("args")
        .map(|v| v.cloned().collect())
        .unwrap_or_default();

    if pid_mode {
        // Get or set mode for existing PIDs
        // chrt -p <pid>          -> query
        // chrt -p <priority> <pid>  -> set
        if positional.is_empty() {
            return Err(USimpleError::new(1, "missing PID argument"));
        }

        let is_setting = policy.is_some() && positional.len() >= 2;

        if is_setting {
            let priority: u32 = positional[0]
                .to_string_lossy()
                .parse()
                .map_err(|_| USimpleError::new(1, "invalid priority"))?;
            let pid: i32 = positional[1]
                .to_string_lossy()
                .parse()
                .map_err(|_| USimpleError::new(1, "invalid PID"))?;

            let pids = if all_tasks { get_tids(pid) } else { vec![pid] };
            for tid in pids {
                set_sched(
                    tid,
                    policy.unwrap(),
                    priority,
                    reset_on_fork,
                    sched_runtime,
                    sched_period,
                    sched_deadline,
                )?;
            }

            if verbose {
                print_sched_info(pid)?;
            }
        } else {
            // Query mode
            let pid: i32 = positional[0]
                .to_string_lossy()
                .parse()
                .map_err(|_| USimpleError::new(1, "invalid PID"))?;

            let pids = if all_tasks { get_tids(pid) } else { vec![pid] };
            for tid in pids {
                print_sched_info(tid)?;
            }
        }

        return Ok(());
    }

    // Command mode: chrt [policy] <priority> <command> [args...]
    if positional.is_empty() {
        return Err(USimpleError::new(1, "missing priority or command"));
    }

    let priority: u32 = positional[0]
        .to_string_lossy()
        .parse()
        .map_err(|_| USimpleError::new(1, "invalid priority"))?;

    if positional.len() < 2 {
        return Err(USimpleError::new(1, "missing command"));
    }

    let policy = policy.unwrap_or(libc::SCHED_RR);

    // Set scheduling policy on ourselves before exec
    set_sched(
        0,
        policy,
        priority,
        reset_on_fork,
        sched_runtime,
        sched_period,
        sched_deadline,
    )?;

    let program = &positional[1];
    let args = &positional[2..];

    if verbose {
        print_sched_info(0)?;
    }

    let err = process::Command::new(program).args(args).exec();

    let code = match err.kind() {
        io::ErrorKind::NotFound => 127,
        io::ErrorKind::PermissionDenied => 126,
        _ => 1,
    };
    Err(USimpleError::new(
        code,
        format!("failed to execute {}: {err}", program.to_string_lossy()),
    ))
}

fn set_sched(
    pid: i32,
    policy: i32,
    priority: u32,
    reset_on_fork: bool,
    runtime: Option<u64>,
    period: Option<u64>,
    deadline: Option<u64>,
) -> UResult<()> {
    let mut attr: libc::sched_attr = unsafe { std::mem::zeroed() };
    attr.size = std::mem::size_of::<libc::sched_attr>() as u32;
    attr.sched_policy = policy as u32;
    attr.sched_priority = priority;

    if reset_on_fork {
        attr.sched_flags |= libc::SCHED_FLAG_RESET_ON_FORK as u64;
    }

    if let Some(rt) = runtime {
        attr.sched_runtime = rt;
    }
    if let Some(p) = period {
        attr.sched_period = p;
    }
    if let Some(d) = deadline {
        attr.sched_deadline = d;
    }

    sched_setattr(pid, &attr)
        .map_err(|e| USimpleError::new(1, format!("failed to set pid {pid}'s policy: {e}")))
}

pub fn uu_app() -> Command {
    Command::new(uucore::util_name())
        .version(crate_version!())
        .about(ABOUT)
        .override_usage(format_usage(USAGE))
        .infer_long_args(true)
        // Policy options
        .arg(
            Arg::new("batch")
                .short('b')
                .long("batch")
                .help("set policy to SCHED_BATCH")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("deadline")
                .short('d')
                .long("deadline")
                .help("set policy to SCHED_DEADLINE")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("fifo")
                .short('f')
                .long("fifo")
                .help("set policy to SCHED_FIFO")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("idle")
                .short('i')
                .long("idle")
                .help("set policy to SCHED_IDLE")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("other")
                .short('o')
                .long("other")
                .help("set policy to SCHED_OTHER")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("rr")
                .short('r')
                .long("rr")
                .help("set policy to SCHED_RR (default)")
                .action(ArgAction::SetTrue),
        )
        .group(
            ArgGroup::new("policy")
                .args(["batch", "deadline", "fifo", "idle", "other", "rr"])
                .multiple(false),
        )
        // Scheduling options
        .arg(
            Arg::new("reset-on-fork")
                .short('R')
                .long("reset-on-fork")
                .help("set reset-on-fork flag")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("sched-runtime")
                .short('T')
                .long("sched-runtime")
                .help("runtime parameter for DEADLINE (ns)")
                .value_name("ns")
                .action(ArgAction::Set),
        )
        .arg(
            Arg::new("sched-period")
                .short('P')
                .long("sched-period")
                .help("period parameter for DEADLINE (ns)")
                .value_name("ns")
                .action(ArgAction::Set),
        )
        .arg(
            Arg::new("sched-deadline")
                .short('D')
                .long("sched-deadline")
                .help("deadline parameter for DEADLINE (ns)")
                .value_name("ns")
                .action(ArgAction::Set),
        )
        // Other options
        .arg(
            Arg::new("all-tasks")
                .short('a')
                .long("all-tasks")
                .help("operate on all tasks (threads) for a given pid")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("max")
                .short('m')
                .long("max")
                .help("show min and max valid priorities")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("pid")
                .short('p')
                .long("pid")
                .help("operate on existing given pid")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .help("display status information")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("args")
                .help("priority, command, and arguments")
                .index(1)
                .action(ArgAction::Set)
                .trailing_var_arg(true)
                .value_parser(clap::builder::ValueParser::os_string())
                .num_args(1..),
        )
}
