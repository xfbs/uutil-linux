// This file is part of the uutils util-linux package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use clap::{crate_version, Arg, ArgAction, Command};
use std::ffi::OsString;
use std::io;
use std::os::unix::process::CommandExt;
use std::process;
use uucore::error::{UResult, USimpleError};
use uucore::{format_usage, help_about, help_usage};

const ABOUT: &str = help_about!("ionice.md");
const USAGE: &str = help_usage!("ionice.md");

// ioprio constants from linux/ioprio.h
const IOPRIO_CLASS_SHIFT: u32 = 13;
const IOPRIO_CLASS_NONE: u32 = 0;
const IOPRIO_CLASS_RT: u32 = 1;
const IOPRIO_CLASS_BE: u32 = 2;
const IOPRIO_CLASS_IDLE: u32 = 3;

const IOPRIO_WHO_PROCESS: i32 = 1;
const IOPRIO_WHO_PGRP: i32 = 2;
const IOPRIO_WHO_USER: i32 = 3;

fn ioprio_value(class: u32, data: u32) -> u32 {
    (class << IOPRIO_CLASS_SHIFT) | data
}

fn ioprio_class(ioprio: u32) -> u32 {
    ioprio >> IOPRIO_CLASS_SHIFT
}

fn ioprio_data(ioprio: u32) -> u32 {
    ioprio & ((1 << IOPRIO_CLASS_SHIFT) - 1)
}

fn ioprio_get(which: i32, who: i32) -> io::Result<u32> {
    let ret = unsafe { libc::syscall(libc::SYS_ioprio_get, which, who) };
    if ret < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(ret as u32)
    }
}

fn ioprio_set(which: i32, who: i32, ioprio: u32) -> io::Result<()> {
    let ret = unsafe { libc::syscall(libc::SYS_ioprio_set, which, who, ioprio) };
    if ret < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn class_name(class: u32) -> &'static str {
    match class {
        IOPRIO_CLASS_NONE => "none",
        IOPRIO_CLASS_RT => "realtime",
        IOPRIO_CLASS_BE => "best-effort",
        IOPRIO_CLASS_IDLE => "idle",
        _ => "unknown",
    }
}

fn parse_class(s: &str) -> Result<u32, String> {
    match s {
        "0" | "none" => Ok(IOPRIO_CLASS_NONE),
        "1" | "realtime" | "rt" => Ok(IOPRIO_CLASS_RT),
        "2" | "best-effort" | "be" => Ok(IOPRIO_CLASS_BE),
        "3" | "idle" => Ok(IOPRIO_CLASS_IDLE),
        _ => Err(format!("unknown scheduling class: '{s}'")),
    }
}

#[uucore::main]
pub fn uumain(args: impl uucore::Args) -> UResult<()> {
    let matches = uu_app().try_get_matches_from(args)?;

    let class = matches
        .get_one::<String>("class")
        .map(|s| parse_class(s))
        .transpose()
        .map_err(|e| USimpleError::new(1, e))?;

    let classdata = matches
        .get_one::<String>("classdata")
        .map(|s| {
            s.parse::<u32>()
                .map_err(|_| USimpleError::new(1, format!("invalid class data: '{s}'")))
        })
        .transpose()?;

    let ignore = matches.get_flag("ignore");

    let pids: Vec<i32> = matches
        .get_many::<String>("pid")
        .map(|v| v.map(|s| s.parse::<i32>().unwrap_or(-1)).collect())
        .unwrap_or_default();

    let pgids: Vec<i32> = matches
        .get_many::<String>("pgid")
        .map(|v| v.map(|s| s.parse::<i32>().unwrap_or(-1)).collect())
        .unwrap_or_default();

    let uids: Vec<i32> = matches
        .get_many::<String>("uid")
        .map(|v| v.map(|s| s.parse::<i32>().unwrap_or(-1)).collect())
        .unwrap_or_default();

    let command: Vec<OsString> = matches
        .get_many::<OsString>("command")
        .map(|v| v.cloned().collect())
        .unwrap_or_default();

    let has_ids = !pids.is_empty() || !pgids.is_empty() || !uids.is_empty();
    let is_setting = class.is_some() || classdata.is_some();

    // If no command and no pids, query self
    if command.is_empty() && !has_ids {
        // Query current process
        return print_ioprio(IOPRIO_WHO_PROCESS, 0);
    }

    // Query or set for specific PIDs/PGIDs/UIDs
    if has_ids {
        if is_setting {
            let class = class.unwrap_or(IOPRIO_CLASS_BE);
            let data = classdata.unwrap_or(if class == IOPRIO_CLASS_IDLE { 0 } else { 4 });
            let prio = ioprio_value(class, data);

            for &pid in &pids {
                if let Err(e) = ioprio_set(IOPRIO_WHO_PROCESS, pid, prio) {
                    if !ignore {
                        return Err(USimpleError::new(1, format!("ioprio_set failed: {e}")));
                    }
                    uucore::show_error!("ioprio_set failed: {e}");
                }
            }
            for &pgid in &pgids {
                if let Err(e) = ioprio_set(IOPRIO_WHO_PGRP, pgid, prio) {
                    if !ignore {
                        return Err(USimpleError::new(1, format!("ioprio_set failed: {e}")));
                    }
                    uucore::show_error!("ioprio_set failed: {e}");
                }
            }
            for &uid in &uids {
                if let Err(e) = ioprio_set(IOPRIO_WHO_USER, uid, prio) {
                    if !ignore {
                        return Err(USimpleError::new(1, format!("ioprio_set failed: {e}")));
                    }
                    uucore::show_error!("ioprio_set failed: {e}");
                }
            }
        } else {
            // Query mode
            for &pid in &pids {
                print_ioprio(IOPRIO_WHO_PROCESS, pid)?;
            }
            for &pgid in &pgids {
                print_ioprio(IOPRIO_WHO_PGRP, pgid)?;
            }
            for &uid in &uids {
                print_ioprio(IOPRIO_WHO_USER, uid)?;
            }
        }
        return Ok(());
    }

    // Execute a command with the specified I/O priority
    let class = class.unwrap_or(IOPRIO_CLASS_BE);
    let data = classdata.unwrap_or(if class == IOPRIO_CLASS_IDLE { 0 } else { 4 });
    let prio = ioprio_value(class, data);

    if let Err(e) = ioprio_set(IOPRIO_WHO_PROCESS, 0, prio) {
        if !ignore {
            return Err(USimpleError::new(1, format!("ioprio_set failed: {e}")));
        }
        uucore::show_error!("ioprio_set failed: {e}");
    }

    let program = &command[0];
    let args = &command[1..];
    let err = process::Command::new(program).args(args).exec();

    // exec only returns on error
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

fn print_ioprio(which: i32, who: i32) -> UResult<()> {
    let ioprio = ioprio_get(which, who)
        .map_err(|e| USimpleError::new(1, format!("ioprio_get failed: {e}")))?;

    let class = ioprio_class(ioprio);
    let data = ioprio_data(ioprio);

    println!("{}: prio {data}", class_name(class));
    Ok(())
}

pub fn uu_app() -> Command {
    Command::new(uucore::util_name())
        .version(crate_version!())
        .about(ABOUT)
        .override_usage(format_usage(USAGE))
        .infer_long_args(true)
        .arg(
            Arg::new("class")
                .short('c')
                .long("class")
                .help("scheduling class (0=none, 1=realtime, 2=best-effort, 3=idle)")
                .value_name("class")
                .action(ArgAction::Set),
        )
        .arg(
            Arg::new("classdata")
                .short('n')
                .long("classdata")
                .help("scheduling class data (priority level 0-7)")
                .value_name("level")
                .action(ArgAction::Set),
        )
        .arg(
            Arg::new("pid")
                .short('p')
                .long("pid")
                .help("process ID(s)")
                .value_name("PID")
                .action(ArgAction::Set)
                .num_args(1..),
        )
        .arg(
            Arg::new("pgid")
                .short('P')
                .long("pgid")
                .help("process group ID(s)")
                .value_name("PGID")
                .action(ArgAction::Set)
                .num_args(1..),
        )
        .arg(
            Arg::new("uid")
                .short('u')
                .long("uid")
                .help("user ID(s)")
                .value_name("UID")
                .action(ArgAction::Set)
                .num_args(1..),
        )
        .arg(
            Arg::new("ignore")
                .short('t')
                .long("ignore")
                .help("ignore failure to set priority")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("command")
                .help("command to execute")
                .index(1)
                .action(ArgAction::Set)
                .trailing_var_arg(true)
                .value_parser(clap::builder::ValueParser::os_string())
                .num_args(1..),
        )
}
