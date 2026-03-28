// This file is part of the uutils util-linux package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use clap::{crate_version, Arg, ArgAction, Command};
use std::fs;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};
use uucore::error::{UResult, USimpleError};
use uucore::{format_usage, help_about, help_usage};

const ABOUT: &str = help_about!("namei.md");
const USAGE: &str = help_usage!("namei.md");

// Linux default symlink follow limit
const MAXSYMLINKS: usize = 40;

mod options {
    pub const LONG: &str = "long";
    pub const MODES: &str = "modes";
    pub const NOSYMLINKS: &str = "nosymlinks";
    pub const OWNERS: &str = "owners";
    pub const VERTICAL: &str = "vertical";
    pub const MOUNTPOINTS: &str = "mountpoints";
    pub const PATHNAME: &str = "pathname";
}

struct Config {
    modes: bool,
    owners: bool,
    vertical: bool,
    nosymlinks: bool,
    mountpoints: bool,
}

#[uucore::main]
pub fn uumain(args: impl uucore::Args) -> UResult<()> {
    let matches = uu_app().try_get_matches_from(args)?;

    let pathnames: Vec<&String> = matches
        .get_many::<String>(options::PATHNAME)
        .ok_or_else(|| USimpleError::new(1, "pathname argument is missing"))?
        .collect();

    let long = matches.get_flag(options::LONG);
    let config = Config {
        modes: long || matches.get_flag(options::MODES),
        owners: long || matches.get_flag(options::OWNERS),
        vertical: long || matches.get_flag(options::VERTICAL),
        nosymlinks: matches.get_flag(options::NOSYMLINKS),
        mountpoints: matches.get_flag(options::MOUNTPOINTS),
    };

    let mut exit_code = 0;

    for pathname in &pathnames {
        if let Err(_) = follow_path(pathname, &config) {
            exit_code = 1;
        }
    }

    if exit_code != 0 {
        Err(USimpleError::new(exit_code, ""))
    } else {
        Ok(())
    }
}

fn follow_path(pathname: &str, config: &Config) -> Result<(), ()> {
    println!("f: {pathname}");

    let components = split_path(pathname);
    let mut current_path = PathBuf::new();
    let mut ok = true;

    for component in &components {
        if current_path.as_os_str().is_empty() {
            if component == "/" {
                current_path.push("/");
            } else {
                current_path.push(component);
            }
        } else {
            current_path.push(component);
        }

        if !print_component(&current_path, component, config, 0, &mut 0) {
            ok = false;
            break;
        }
    }

    if ok { Ok(()) } else { Err(()) }
}

/// Split a pathname into components, preserving "/" as a leading component
/// for absolute paths. Skips empty components (double slashes).
fn split_path(pathname: &str) -> Vec<String> {
    let mut components = Vec::new();

    if pathname.starts_with('/') {
        components.push("/".to_string());
    }

    for part in pathname.split('/') {
        if !part.is_empty() {
            components.push(part.to_string());
        }
    }

    components
}

/// Print info for one path component. Returns false if we hit an error.
fn print_component(
    full_path: &Path,
    name: &str,
    config: &Config,
    indent: usize,
    symlink_count: &mut usize,
) -> bool {
    // Use lstat (don't follow symlinks)
    let meta = match fs::symlink_metadata(full_path) {
        Ok(m) => m,
        Err(e) => {
            let indent_str = "  ".repeat(indent);
            let msg = os_error_string(&e);
            // Match util-linux error format: indent + 4 spaces + name + " - " + message
            println!("{indent_str}    {name} - {msg}");
            return false;
        }
    };

    let file_type = meta.file_type();
    let type_char = if file_type.is_symlink() {
        'l'
    } else if file_type.is_dir() {
        if config.mountpoints && is_mountpoint(full_path) {
            'D'
        } else {
            'd'
        }
    } else if file_type.is_file() {
        '-'
    } else if file_type.is_block_device() {
        'b'
    } else if file_type.is_char_device() {
        'c'
    } else if is_socket(&meta) {
        's'
    } else if is_fifo(&meta) {
        'p'
    } else {
        '?'
    };

    let indent_str = "  ".repeat(indent);

    // Build the prefix (mode/type + owner) and the name separately.
    // In vertical mode, indent goes between prefix and name (keeps columns aligned).
    // In non-vertical mode, indent goes before everything.
    let prefix = if config.modes {
        format_mode(type_char, meta.mode())
    } else {
        format!("{type_char}")
    };

    let owner_str = if config.owners {
        format_owner(meta.uid(), meta.gid())
    } else {
        String::new()
    };

    // If it's a symlink, show with arrow and target
    if file_type.is_symlink() {
        let target = match fs::read_link(full_path) {
            Ok(t) => t,
            Err(e) => {
                let msg = os_error_string(&e);
                print_line(config, &indent_str, &prefix, &owner_str, &format!("{name} - {msg}"));
                return false;
            }
        };

        let target_str = target.to_string_lossy();
        print_line(config, &indent_str, &prefix, &owner_str, &format!("{name} -> {target_str}"));

        if config.nosymlinks {
            return true;
        }

        *symlink_count += 1;
        if *symlink_count > MAXSYMLINKS {
            return true;
        }

        // Resolve the symlink target: if absolute, walk from root;
        // if relative, walk relative to the symlink's parent directory.
        let base_dir = full_path.parent().unwrap_or(Path::new("/"));

        if target.is_absolute() {
            let target_components = split_path(&target_str);
            let mut target_current = PathBuf::new();

            for component in &target_components {
                if target_current.as_os_str().is_empty() && component == "/" {
                    target_current.push("/");
                } else {
                    target_current.push(component);
                }

                if !print_component(
                    &target_current,
                    component,
                    config,
                    indent + 1,
                    symlink_count,
                ) {
                    return false;
                }
            }
        } else {
            let target_components = split_path(&target_str);
            let mut target_current = base_dir.to_path_buf();

            for component in &target_components {
                target_current.push(component);

                if !print_component(
                    &target_current,
                    component,
                    config,
                    indent + 1,
                    symlink_count,
                ) {
                    return false;
                }
            }
        }
    } else {
        print_line(config, &indent_str, &prefix, &owner_str, name);
    }

    true
}

/// Print a formatted line.
/// In vertical mode: `{prefix}{owner} {indent}{name}` (columns stay aligned)
/// In non-vertical mode: `{indent} {prefix}{owner} {name}`
fn print_line(config: &Config, indent: &str, prefix: &str, owner: &str, name: &str) {
    if config.vertical {
        println!("{prefix}{owner} {indent}{name}");
    } else {
        println!("{indent} {prefix}{owner} {name}");
    }
}

fn format_mode(type_char: char, mode: u32) -> String {
    let mut s = String::with_capacity(11);
    s.push(type_char);
    s.push(if mode & 0o400 != 0 { 'r' } else { '-' });
    s.push(if mode & 0o200 != 0 { 'w' } else { '-' });
    s.push(match (mode & 0o100 != 0, mode & 0o4000 != 0) {
        (true, true) => 's',
        (true, false) => 'x',
        (false, true) => 'S',
        (false, false) => '-',
    });
    s.push(if mode & 0o040 != 0 { 'r' } else { '-' });
    s.push(if mode & 0o020 != 0 { 'w' } else { '-' });
    s.push(match (mode & 0o010 != 0, mode & 0o2000 != 0) {
        (true, true) => 's',
        (true, false) => 'x',
        (false, true) => 'S',
        (false, false) => '-',
    });
    s.push(if mode & 0o004 != 0 { 'r' } else { '-' });
    s.push(if mode & 0o002 != 0 { 'w' } else { '-' });
    s.push(match (mode & 0o001 != 0, mode & 0o1000 != 0) {
        (true, true) => 't',
        (true, false) => 'x',
        (false, true) => 'T',
        (false, false) => '-',
    });
    s
}

fn format_owner(uid: u32, gid: u32) -> String {
    let user = lookup_user(uid);
    let group = lookup_group(gid);
    format!(" {user} {group}")
}

fn lookup_user(uid: u32) -> String {
    unsafe {
        let pw = libc::getpwuid(uid);
        if pw.is_null() {
            return uid.to_string();
        }
        std::ffi::CStr::from_ptr((*pw).pw_name)
            .to_string_lossy()
            .into_owned()
    }
}

fn lookup_group(gid: u32) -> String {
    unsafe {
        let gr = libc::getgrgid(gid);
        if gr.is_null() {
            return gid.to_string();
        }
        std::ffi::CStr::from_ptr((*gr).gr_name)
            .to_string_lossy()
            .into_owned()
    }
}

/// Check if a directory is a mountpoint by comparing its device ID
/// with its parent's device ID. Root is always a mountpoint.
fn is_mountpoint(path: &Path) -> bool {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return false;
    };
    if !meta.is_dir() {
        return false;
    }
    let parent = path.join("..");
    let Ok(parent_meta) = fs::metadata(&parent) else {
        return false;
    };
    // Root directory: parent is itself, so inode will be the same
    if meta.ino() == parent_meta.ino() && meta.dev() == parent_meta.dev() {
        return true;
    }
    meta.dev() != parent_meta.dev()
}

/// Get the OS error message (like strerror), without Rust's "(os error N)" suffix.
fn os_error_string(e: &std::io::Error) -> String {
    if let Some(errno) = e.raw_os_error() {
        let msg = unsafe { libc::strerror(errno) };
        if !msg.is_null() {
            return unsafe { std::ffi::CStr::from_ptr(msg) }
                .to_string_lossy()
                .into_owned();
        }
    }
    e.to_string()
}

fn is_socket(meta: &fs::Metadata) -> bool {
    (meta.mode() & libc::S_IFMT as u32) == libc::S_IFSOCK as u32
}

fn is_fifo(meta: &fs::Metadata) -> bool {
    (meta.mode() & libc::S_IFMT as u32) == libc::S_IFIFO as u32
}

pub fn uu_app() -> Command {
    Command::new(uucore::util_name())
        .version(crate_version!())
        .about(ABOUT)
        .override_usage(format_usage(USAGE))
        .infer_long_args(true)
        .arg(
            Arg::new(options::LONG)
                .short('l')
                .long("long")
                .help("use the long listing format (same as -m -o -v)")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::MODES)
                .short('m')
                .long("modes")
                .help("show the mode bits of each file type")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::NOSYMLINKS)
                .short('n')
                .long("nosymlinks")
                .help("don't follow symlinks")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::OWNERS)
                .short('o')
                .long("owners")
                .help("show owner and group name of each file")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::VERTICAL)
                .short('v')
                .long("vertical")
                .help("vertically align the modes and owners")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::MOUNTPOINTS)
                .short('x')
                .long("mountpoints")
                .help("show mountpoint directories with a 'D' rather than a 'd'")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(options::PATHNAME)
                .value_name("pathname")
                .help("pathnames to follow")
                .required(true)
                .action(ArgAction::Set)
                .num_args(1..),
        )
}
