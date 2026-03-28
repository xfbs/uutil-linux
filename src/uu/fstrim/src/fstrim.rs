// This file is part of the uutils util-linux package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use clap::{crate_version, Arg, ArgAction, ArgGroup, Command};
use std::fs::{self, File};
use std::io::{self, BufRead};
use std::os::fd::AsRawFd;
use std::path::Path;
use uucore::error::{UResult, USimpleError};
use uucore::parser::parse_size;
use uucore::{format_usage, help_about, help_usage};

const ABOUT: &str = help_about!("fstrim.md");
const USAGE: &str = help_usage!("fstrim.md");

// FITRIM ioctl number: _IOWR('X', 121, struct fstrim_range)
// This is the same value across all Linux architectures
const FITRIM: libc::c_ulong = 0xC0185879;

#[repr(C)]
struct FstrimRange {
    start: u64,
    len: u64,
    minlen: u64,
}

mod options {
    pub const ALL: &str = "all";
    pub const FSTAB: &str = "fstab";
    pub const LISTED_IN: &str = "listed-in";
    pub const OFFSET: &str = "offset";
    pub const LENGTH: &str = "length";
    pub const MINIMUM: &str = "minimum";
    pub const TYPES: &str = "types";
    pub const VERBOSE: &str = "verbose";
    pub const QUIET_UNSUPPORTED: &str = "quiet-unsupported";
    pub const DRY_RUN: &str = "dry-run";
    pub const MOUNTPOINT: &str = "mountpoint";
}

fn parse_size_arg(s: &str) -> Result<u64, String> {
    parse_size::parse_size_u64(s).map_err(|e| format!("failed to parse size: {e}"))
}

#[cfg(target_os = "linux")]
#[uucore::main]
pub fn uumain(args: impl uucore::Args) -> UResult<()> {
    let matches = uu_app().try_get_matches_from(args)?;

    let verbose = matches.get_flag(options::VERBOSE);
    let dry_run = matches.get_flag(options::DRY_RUN);
    let quiet_unsupported = matches.get_flag(options::QUIET_UNSUPPORTED);

    let offset = matches
        .get_one::<String>(options::OFFSET)
        .map(|s| parse_size_arg(s))
        .transpose()
        .map_err(|e| USimpleError::new(1, e))?
        .unwrap_or(0);

    let length = matches
        .get_one::<String>(options::LENGTH)
        .map(|s| parse_size_arg(s))
        .transpose()
        .map_err(|e| USimpleError::new(1, e))?
        .unwrap_or(u64::MAX);

    let minimum = matches
        .get_one::<String>(options::MINIMUM)
        .map(|s| parse_size_arg(s))
        .transpose()
        .map_err(|e| USimpleError::new(1, e))?
        .unwrap_or(0);

    let type_filter = matches.get_one::<String>(options::TYPES).cloned();

    let all = matches.get_flag(options::ALL);
    let fstab = matches.get_flag(options::FSTAB);
    let listed_in = matches.get_one::<String>(options::LISTED_IN).cloned();

    if all || fstab || listed_in.is_some() {
        let mountpoints = if let Some(ref list) = listed_in {
            get_mountpoints_from_listed(list)?
        } else if fstab {
            get_mountpoints_from_fstab()?
        } else {
            get_all_mountpoints()?
        };

        let mut any_success = false;
        let mut any_failure = false;

        for mp in &mountpoints {
            if !should_trim_fs(mp, &type_filter, fstab) {
                continue;
            }

            match do_fstrim(&mp.mountpoint, offset, length, minimum, verbose, dry_run) {
                Ok(()) => any_success = true,
                Err(e) => {
                    if is_silent_error(&e, quiet_unsupported) {
                        // Silently ignored errors don't count as failures
                    } else {
                        eprintln!("fstrim: {}: {e}", mp.mountpoint);
                        any_failure = true;
                    }
                }
            }
        }

        if any_failure && !any_success {
            uucore::error::set_exit_code(32);
        } else if any_failure && any_success {
            uucore::error::set_exit_code(64);
        }

        Ok(())
    } else {
        let mountpoint = matches
            .get_one::<String>(options::MOUNTPOINT)
            .ok_or_else(|| USimpleError::new(1, "no mountpoint specified"))?;

        do_fstrim(mountpoint, offset, length, minimum, verbose, dry_run).map_err(|e| {
            if quiet_unsupported && is_unsupported_error(&e) {
                // Suppress and exit cleanly
                return USimpleError::new(0, "");
            }
            USimpleError::new(1, format!("{mountpoint}: {e}"))
        })
    }
}

fn do_fstrim(
    mountpoint: &str,
    offset: u64,
    length: u64,
    minimum: u64,
    verbose: bool,
    dry_run: bool,
) -> Result<(), io::Error> {
    let file = File::open(mountpoint)?;
    let metadata = file.metadata()?;
    if !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a directory",
        ));
    }

    let mut range = FstrimRange {
        start: offset,
        len: length,
        minlen: minimum,
    };

    if !dry_run {
        let ret = unsafe {
            libc::ioctl(
                file.as_raw_fd(),
                FITRIM,
                &mut range as *mut FstrimRange,
            )
        };
        if ret < 0 {
            return Err(io::Error::last_os_error());
        }
    }

    if verbose {
        let trimmed = if dry_run { 0 } else { range.len };
        let dry_str = if dry_run { " (dry run)" } else { "" };
        println!(
            "{}: {}{dry_str} trimmed",
            mountpoint,
            humanize_bytes(trimmed)
        );
    }

    Ok(())
}

fn humanize_bytes(bytes: u64) -> String {
    const UNITS: &[(u64, &str)] = &[
        (1 << 60, "EiB"),
        (1 << 50, "PiB"),
        (1 << 40, "TiB"),
        (1 << 30, "GiB"),
        (1 << 20, "MiB"),
        (1 << 10, "KiB"),
    ];

    for &(threshold, unit) in UNITS {
        if bytes >= threshold {
            if bytes % threshold == 0 {
                return format!("{} {}", bytes / threshold, unit);
            }
            return format!("{:.1} {}", bytes as f64 / threshold as f64, unit);
        }
    }
    format!("{bytes} B")
}

struct MountEntry {
    mountpoint: String,
    fstype: String,
    source: String,
    options: String,
}

/// Parse /proc/self/mountinfo to get all mounted filesystems.
fn get_all_mountpoints() -> UResult<Vec<MountEntry>> {
    parse_mountinfo("/proc/self/mountinfo")
}

/// Parse /etc/fstab to get filesystem entries.
fn get_mountpoints_from_fstab() -> UResult<Vec<MountEntry>> {
    parse_fstab("/etc/fstab")
}

/// Parse files from a colon-separated list.
fn get_mountpoints_from_listed(list: &str) -> UResult<Vec<MountEntry>> {
    for path in list.split(':') {
        if path.is_empty() {
            continue;
        }
        if !Path::new(path).exists() {
            continue;
        }
        let content = fs::read_to_string(path).unwrap_or_default();
        if content.trim().is_empty() {
            continue;
        }

        // Try to detect format: mountinfo has more fields than fstab
        let first_line = content.lines().next().unwrap_or("");
        let fields: Vec<&str> = first_line.split_whitespace().collect();
        if fields.len() > 6 {
            return parse_mountinfo(path);
        } else {
            return parse_fstab(path);
        }
    }

    Ok(Vec::new())
}

fn parse_mountinfo(path: &str) -> UResult<Vec<MountEntry>> {
    let file = File::open(path)
        .map_err(|e| USimpleError::new(1, format!("cannot open {path}: {e}")))?;

    let mut entries = Vec::new();
    for line in io::BufReader::new(file).lines() {
        let line = line.map_err(|e| USimpleError::new(1, format!("read error: {e}")))?;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // mountinfo format:
        // id parent major:minor root mountpoint options shared - fstype source super_options
        let fields: Vec<&str> = line.split_whitespace().collect();

        // Find the separator "-"
        let sep_pos = fields.iter().position(|&f| f == "-");
        let Some(sep) = sep_pos else { continue };

        if fields.len() < sep + 3 {
            continue;
        }

        let mountpoint = fields.get(4).unwrap_or(&"").to_string();
        let options = fields.get(5).unwrap_or(&"").to_string();
        let fstype = fields[sep + 1].to_string();
        let source = fields.get(sep + 2).unwrap_or(&"").to_string();
        let super_options = fields.get(sep + 3).unwrap_or(&"").to_string();

        // Combine options
        let all_options = format!("{options},{super_options}");

        entries.push(MountEntry {
            mountpoint,
            fstype,
            source,
            options: all_options,
        });
    }

    Ok(entries)
}

fn parse_fstab(path: &str) -> UResult<Vec<MountEntry>> {
    let file = File::open(path)
        .map_err(|e| USimpleError::new(1, format!("cannot open {path}: {e}")))?;

    let mut entries = Vec::new();
    for line in io::BufReader::new(file).lines() {
        let line = line.map_err(|e| USimpleError::new(1, format!("read error: {e}")))?;
        let line = line.trim().to_string();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 3 {
            continue;
        }

        entries.push(MountEntry {
            mountpoint: fields[1].to_string(),
            fstype: fields[2].to_string(),
            source: fields[0].to_string(),
            options: fields.get(3).unwrap_or(&"").to_string(),
        });
    }

    Ok(entries)
}

fn should_trim_fs(entry: &MountEntry, type_filter: &Option<String>, is_fstab: bool) -> bool {
    // Skip autofs
    if entry.fstype == "autofs" {
        return false;
    }

    // Only trim filesystems backed by real block devices
    // (source starts with /dev/)
    if !entry.source.starts_with("/dev/") {
        return false;
    }

    // Skip entries with X-fstrim.notrim option (fstab mode)
    if is_fstab && entry.options.contains("X-fstrim.notrim") {
        return false;
    }

    // Apply type filter
    if let Some(ref filter) = type_filter {
        return matches_type_filter(&entry.fstype, filter);
    }

    true
}

fn matches_type_filter(fstype: &str, filter: &str) -> bool {
    let types: Vec<&str> = filter.split(',').collect();

    // Check if this is a negative filter
    let all_negated = types.iter().all(|t| t.starts_with("no"));

    if all_negated {
        // All types are negated: allow everything except listed
        for t in &types {
            let excluded = t.strip_prefix("no").unwrap_or(t);
            if fstype == excluded {
                return false;
            }
        }
        true
    } else {
        // Positive filter: only allow listed types
        types.iter().any(|&t| t == fstype)
    }
}

fn is_unsupported_error(e: &io::Error) -> bool {
    matches!(
        e.raw_os_error(),
        Some(libc::EOPNOTSUPP) | Some(libc::ENOTTY) | Some(libc::EBADF)
    )
}

fn is_silent_error(e: &io::Error, quiet_unsupported: bool) -> bool {
    if is_unsupported_error(e) {
        return true; // Always silent for -a/-A mode
    }
    // Read-only filesystem and permission errors are also silent in -a/-A mode
    if e.raw_os_error() == Some(libc::EROFS) || e.raw_os_error() == Some(libc::EACCES) {
        return true;
    }
    if quiet_unsupported {
        return is_unsupported_error(e);
    }
    false
}

#[cfg(not(target_os = "linux"))]
#[uucore::main]
pub fn uumain(args: impl uucore::Args) -> UResult<()> {
    let _matches = uu_app().try_get_matches_from(args)?;
    Err(USimpleError::new(
        1,
        "`fstrim` is available only on Linux.",
    ))
}

pub fn uu_app() -> Command {
    Command::new(uucore::util_name())
        .version(crate_version!())
        .about(ABOUT)
        .override_usage(format_usage(USAGE))
        .infer_long_args(true)
        .arg(
            Arg::new(options::ALL)
                .short('a')
                .long("all")
                .help("trim all mounted filesystems")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::FSTAB)
                .short('A')
                .long("fstab")
                .help("trim filesystems from /etc/fstab")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::LISTED_IN)
                .short('I')
                .long("listed-in")
                .help("trim filesystems listed in specified files")
                .value_name("list")
                .action(ArgAction::Set),
        )
        .arg(
            Arg::new(options::OFFSET)
                .short('o')
                .long("offset")
                .help("the offset in bytes to start discarding from")
                .value_name("num")
                .action(ArgAction::Set),
        )
        .arg(
            Arg::new(options::LENGTH)
                .short('l')
                .long("length")
                .help("the number of bytes to discard")
                .value_name("num")
                .action(ArgAction::Set),
        )
        .arg(
            Arg::new(options::MINIMUM)
                .short('m')
                .long("minimum")
                .help("the minimum extent length to discard")
                .value_name("num")
                .action(ArgAction::Set),
        )
        .arg(
            Arg::new(options::TYPES)
                .short('t')
                .long("types")
                .help("limit the set of filesystem types")
                .value_name("list")
                .action(ArgAction::Set),
        )
        .arg(
            Arg::new(options::VERBOSE)
                .short('v')
                .long("verbose")
                .help("print number of discarded bytes")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::QUIET_UNSUPPORTED)
                .long("quiet-unsupported")
                .help("suppress error messages if trim unsupported")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::DRY_RUN)
                .short('n')
                .long("dry-run")
                .help("does everything, but trim")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::MOUNTPOINT)
                .value_name("mount point")
                .help("mountpoint of the filesystem to trim")
                .index(1)
                .action(ArgAction::Set),
        )
        .group(
            ArgGroup::new("mode")
                .args([options::ALL, options::FSTAB])
                .multiple(false),
        )
}
