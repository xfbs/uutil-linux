// This file is part of the uutils util-linux package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use clap::{crate_version, Arg, ArgAction, Command};
use std::ffi::OsString;
use std::ffi::CString;
use std::io;
use std::os::fd::RawFd;
use std::os::unix::process::CommandExt;
use std::process;
use std::time::Instant;
use uucore::error::{UResult, USimpleError};
use uucore::{format_usage, help_about, help_usage};

const ABOUT: &str = help_about!("flock.md");
const USAGE: &str = help_usage!("flock.md");

const EX_UNAVAILABLE: i32 = 69; // sysexits.h EX_UNAVAILABLE

#[uucore::main]
pub fn uumain(args: impl uucore::Args) -> UResult<()> {
    let matches = uu_app().try_get_matches_from(args)?;

    let shared = matches.get_flag("shared");
    let unlock = matches.get_flag("unlock");
    let nonblock = matches.get_flag("nonblock");
    let no_fork = matches.get_flag("no-fork");
    let close_fd = matches.get_flag("close");
    let verbose = matches.get_flag("verbose");
    let shell_command = matches.get_one::<String>("command");

    let conflict_exit_code = matches
        .get_one::<String>("conflict-exit-code")
        .map(|s| {
            s.parse::<i32>()
                .map_err(|_| USimpleError::new(1, format!("invalid exit code: '{s}'")))
        })
        .transpose()?
        .unwrap_or(1);

    let timeout: Option<f64> = matches
        .get_one::<String>("timeout")
        .map(|s| {
            s.parse::<f64>()
                .map_err(|_| USimpleError::new(1, format!("invalid timeout: '{s}'")))
        })
        .transpose()?;

    let file_arg = matches
        .get_one::<String>("file")
        .ok_or_else(|| USimpleError::new(EX_UNAVAILABLE, "no file or directory specified"))?;

    let command_args: Vec<OsString> = matches
        .get_many::<OsString>("arguments")
        .map(|v| v.cloned().collect())
        .unwrap_or_default();

    if no_fork && close_fd {
        return Err(USimpleError::new(
            1,
            "the --no-fork and --close options are incompatible",
        ));
    }

    // Determine if we're in fd mode (file_arg is a number)
    let (fd, _owned_fd) = if let Ok(fd_num) = file_arg.parse::<RawFd>() {
        // FD mode: use the given file descriptor directly
        (fd_num, false)
    } else {
        // File mode: open/create the file with O_RDONLY|O_CREAT|O_NOCTTY
        let c_path = CString::new(file_arg.as_bytes()).map_err(|_| {
            USimpleError::new(EX_UNAVAILABLE, format!("invalid path: {file_arg}"))
        })?;
        let fd = unsafe {
            libc::open(
                c_path.as_ptr(),
                libc::O_RDONLY | libc::O_CREAT | libc::O_NOCTTY,
                0o666,
            )
        };
        if fd < 0 {
            let err = io::Error::last_os_error();
            return Err(USimpleError::new(
                EX_UNAVAILABLE,
                format!("cannot open lock file {file_arg}: {err}"),
            ));
        }
        (fd, true)
    };

    // Build the flock operation
    let mut operation = if unlock {
        libc::LOCK_UN
    } else if shared {
        libc::LOCK_SH
    } else {
        libc::LOCK_EX
    };

    let use_nonblock = nonblock || timeout.is_some_and(|t| t == 0.0);
    if use_nonblock {
        operation |= libc::LOCK_NB;
    }

    // Acquire the lock
    let start = Instant::now();

    if let Some(secs) = timeout {
        if secs > 0.0 {
            // Timeout mode: try nonblocking in a loop with a timer
            // Use a signal alarm approach or poll
            operation |= libc::LOCK_NB;
            let deadline = std::time::Duration::from_secs_f64(secs);
            loop {
                let ret = unsafe { libc::flock(fd, operation) };
                if ret == 0 {
                    break;
                }
                let err = io::Error::last_os_error();
                if err.raw_os_error() != Some(libc::EWOULDBLOCK) {
                    return Err(USimpleError::new(
                        EX_UNAVAILABLE,
                        format!("flock failed: {err}"),
                    ));
                }
                if start.elapsed() >= deadline {
                    if verbose {
                        eprintln!("flock: timeout, giving up");
                    }
                    uucore::error::set_exit_code(conflict_exit_code);
                    return Ok(());
                }
                // Sleep briefly before retrying
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        } else {
            // timeout == 0 means nonblock
            let ret = unsafe { libc::flock(fd, operation) };
            if ret != 0 {
                let err = io::Error::last_os_error();
                if err.raw_os_error() == Some(libc::EWOULDBLOCK) {
                    if verbose {
                        eprintln!("flock: lock not acquired, giving up");
                    }
                    uucore::error::set_exit_code(conflict_exit_code);
                    return Ok(());
                }
                return Err(USimpleError::new(
                    EX_UNAVAILABLE,
                    format!("flock failed: {err}"),
                ));
            }
        }
    } else {
        let ret = unsafe { libc::flock(fd, operation) };
        if ret != 0 {
            let err = io::Error::last_os_error();
            if use_nonblock && err.raw_os_error() == Some(libc::EWOULDBLOCK) {
                if verbose {
                    eprintln!("flock: lock not acquired, giving up");
                }
                uucore::error::set_exit_code(conflict_exit_code);
                return Ok(());
            }
            return Err(USimpleError::new(
                EX_UNAVAILABLE,
                format!("flock failed: {err}"),
            ));
        }
    }

    if verbose {
        let elapsed = start.elapsed();
        eprintln!(
            "flock: getting lock took {:.6} seconds",
            elapsed.as_secs_f64()
        );
    }

    // If no command, just hold the lock (fd mode with -u, or just lock and exit)
    let has_command = shell_command.is_some() || !command_args.is_empty();
    if !has_command {
        return Ok(());
    }

    // Build the command to execute
    let (program, args): (OsString, Vec<OsString>) = if let Some(cmd) = shell_command {
        // -c mode: run via /bin/sh -c
        ("/bin/sh".into(), vec!["-c".into(), cmd.into()])
    } else {
        let program = command_args[0].clone();
        let args = command_args[1..].to_vec();
        (program, args)
    };

    if verbose {
        eprintln!("flock: executing {}", program.to_string_lossy());
    }

    if close_fd {
        // Close the lock fd before exec - the lock will be released
        // when the fd is closed, but we've already acquired it
        unsafe { libc::close(fd) };
    }

    if no_fork {
        // Replace this process with the command
        let mut cmd = process::Command::new(&program);
        cmd.args(&args);
        let err = cmd.exec();
        return Err(USimpleError::new(
            EX_UNAVAILABLE,
            format!("failed to execute {}: {err}", program.to_string_lossy()),
        ));
    }

    // Fork and exec
    let mut cmd = process::Command::new(&program);
    cmd.args(&args);

    // Close the lock fd in the child if --close was specified
    // (already handled above for no-fork case)
    if close_fd {
        // fd already closed above
    } else {
        // Don't pass the lock fd to the child as a leak - set CLOEXEC
        // Actually, we want the child to inherit the fd so the lock is held
        // while the child runs. The lock fd will be closed when the parent
        // waits and then exits.
    }

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            let code = match e.kind() {
                io::ErrorKind::NotFound => 127,
                io::ErrorKind::PermissionDenied => 126,
                _ => EX_UNAVAILABLE,
            };
            return Err(USimpleError::new(
                code,
                format!("failed to execute {}: {e}", program.to_string_lossy()),
            ));
        }
    };

    match child.wait() {
        Ok(status) => {
            uucore::error::set_exit_code(status.code().unwrap_or(1));
            Ok(())
        }
        Err(e) => Err(USimpleError::new(
            EX_UNAVAILABLE,
            format!("failed to wait for child: {e}"),
        )),
    }
}

pub fn uu_app() -> Command {
    Command::new(uucore::util_name())
        .version(crate_version!())
        .about(ABOUT)
        .override_usage(format_usage(USAGE))
        .infer_long_args(true)
        .arg(
            Arg::new("shared")
                .short('s')
                .long("shared")
                .help("get a shared lock")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("exclusive")
                .short('x')
                .short_alias('e')
                .long("exclusive")
                .help("get an exclusive lock (default)")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("unlock")
                .short('u')
                .long("unlock")
                .help("remove a lock")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("nonblock")
                .short('n')
                .long("nonblock")
                .visible_alias("nb")
                .help("fail rather than wait")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("timeout")
                .short('w')
                .long("timeout")
                .visible_alias("wait")
                .help("wait for a limited amount of time")
                .value_name("secs")
                .action(ArgAction::Set),
        )
        .arg(
            Arg::new("conflict-exit-code")
                .short('E')
                .long("conflict-exit-code")
                .help("exit code after conflict or timeout")
                .value_name("number")
                .action(ArgAction::Set),
        )
        .arg(
            Arg::new("close")
                .short('o')
                .long("close")
                .help("close file descriptor before running command")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("no-fork")
                .short('F')
                .long("no-fork")
                .help("execute command without forking")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("command")
                .short('c')
                .long("command")
                .help("run a single command string through the shell")
                .value_name("command")
                .action(ArgAction::Set),
        )
        .arg(
            Arg::new("verbose")
                .long("verbose")
                .help("increase verbosity")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("file")
                .help("file, directory, or file descriptor number")
                .required(true)
                .index(1)
                .action(ArgAction::Set),
        )
        .arg(
            Arg::new("arguments")
                .help("command and arguments")
                .index(2)
                .action(ArgAction::Set)
                .trailing_var_arg(true)
                .value_parser(clap::builder::ValueParser::os_string())
                .num_args(1..),
        )
}
