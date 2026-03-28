// This file is part of the uutils util-linux package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use clap::{crate_version, Arg, ArgAction, Command};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use uucore::error::{UResult, USimpleError};
use uucore::{format_usage, help_about, help_usage};

const ABOUT: &str = help_about!("whereis.md");
const USAGE: &str = help_usage!("whereis.md");

/// Hard-coded binary search directories (checked if they exist)
const BIN_PATHS: &[&str] = &[
    "/usr/bin",
    "/usr/sbin",
    "/bin",
    "/sbin",
    "/usr/lib",
    "/usr/lib64",
    "/etc",
    "/usr/games",
    "/usr/local/bin",
    "/usr/local/sbin",
    "/usr/local/etc",
    "/usr/local/lib",
    "/usr/local/games",
    "/usr/include",
    "/usr/local",
    "/usr/libexec",
    "/usr/share",
];

/// Hard-coded man page base directories
const MAN_PATHS: &[&str] = &["/usr/share/man", "/usr/local/share/man", "/usr/local/man"];

/// Hard-coded info page directories
const INFO_PATHS: &[&str] = &["/usr/share/info", "/usr/local/share/info"];

/// Hard-coded source search directories
const SRC_PATHS: &[&str] = &["/usr/src"];

#[derive(Clone)]
struct SearchConfig {
    search_bin: bool,
    search_man: bool,
    search_src: bool,
    unusual_only: bool,
    use_glob: bool,
    custom_bin_paths: Option<Vec<PathBuf>>,
    custom_man_paths: Option<Vec<PathBuf>>,
    custom_src_paths: Option<Vec<PathBuf>>,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            search_bin: true,
            search_man: true,
            search_src: true,
            unusual_only: false,
            use_glob: false,
            custom_bin_paths: None,
            custom_man_paths: None,
            custom_src_paths: None,
        }
    }
}

#[uucore::main]
pub fn uumain(args: impl uucore::Args) -> UResult<()> {
    let args: Vec<String> = args
        .map(|a| a.to_string_lossy().into_owned())
        .collect();

    // Handle --help and --version via clap
    if args.iter().any(|a| a == "--help" || a == "-h") {
        let _ = uu_app().try_get_matches_from(&args)?;
        return Ok(());
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        let _ = uu_app().try_get_matches_from(&args)?;
        return Ok(());
    }

    // Custom parsing because of the complex -B/-M/-S/-f and cumulative flag behavior
    let mut config = SearchConfig::default();
    let mut names: Vec<(String, SearchConfig)> = Vec::new();
    let mut list_paths = false;
    let mut has_restriction = false;

    let mut i = 1; // skip argv[0]
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "-b" => {
                if !has_restriction {
                    config.search_bin = true;
                    config.search_man = false;
                    config.search_src = false;
                    has_restriction = true;
                } else {
                    config.search_bin = true;
                }
            }
            "-m" => {
                if !has_restriction {
                    config.search_bin = false;
                    config.search_man = true;
                    config.search_src = false;
                    has_restriction = true;
                } else {
                    config.search_man = true;
                }
            }
            "-s" => {
                if !has_restriction {
                    config.search_bin = false;
                    config.search_man = false;
                    config.search_src = true;
                    has_restriction = true;
                } else {
                    config.search_src = true;
                }
            }
            "-u" => {
                config.unusual_only = true;
            }
            "-g" => {
                config.use_glob = true;
            }
            "-l" => {
                list_paths = true;
            }
            "-B" => {
                let mut dirs = Vec::new();
                i += 1;
                while i < args.len() && args[i] != "-f" {
                    dirs.push(PathBuf::from(&args[i]));
                    i += 1;
                }
                config.custom_bin_paths = Some(dirs);
            }
            "-M" => {
                let mut dirs = Vec::new();
                i += 1;
                while i < args.len() && args[i] != "-f" {
                    dirs.push(PathBuf::from(&args[i]));
                    i += 1;
                }
                config.custom_man_paths = Some(dirs);
            }
            "-S" => {
                let mut dirs = Vec::new();
                i += 1;
                while i < args.len() && args[i] != "-f" {
                    dirs.push(PathBuf::from(&args[i]));
                    i += 1;
                }
                config.custom_src_paths = Some(dirs);
            }
            "-f" => {
                // -f just terminates the directory list, handled above
            }
            _ if arg.starts_with('-') => {
                // Could be combined flags like -bm
                let chars: Vec<char> = arg[1..].chars().collect();
                let mut first_restriction = true;
                for c in chars {
                    match c {
                        'b' => {
                            if first_restriction && !has_restriction {
                                config.search_bin = true;
                                config.search_man = false;
                                config.search_src = false;
                                has_restriction = true;
                                first_restriction = false;
                            } else {
                                config.search_bin = true;
                            }
                        }
                        'm' => {
                            if first_restriction && !has_restriction {
                                config.search_bin = false;
                                config.search_man = true;
                                config.search_src = false;
                                has_restriction = true;
                                first_restriction = false;
                            } else {
                                config.search_man = true;
                            }
                        }
                        's' => {
                            if first_restriction && !has_restriction {
                                config.search_bin = false;
                                config.search_man = false;
                                config.search_src = true;
                                has_restriction = true;
                                first_restriction = false;
                            } else {
                                config.search_src = true;
                            }
                        }
                        'u' => config.unusual_only = true,
                        'g' => config.use_glob = true,
                        'l' => list_paths = true,
                        _ => {
                            return Err(USimpleError::new(
                                1,
                                format!("invalid option -- '{c}'"),
                            ))
                        }
                    }
                }
            }
            _ => {
                // It's a name to search for
                names.push((arg.clone(), config.clone()));
                // Reset restriction flags for next name
                has_restriction = false;
                config.search_bin = true;
                config.search_man = true;
                config.search_src = true;
                config.unusual_only = false;
                config.use_glob = false;
            }
        }
        i += 1;
    }

    if list_paths {
        print_search_paths(&config);
        if names.is_empty() {
            return Ok(());
        }
    }

    if names.is_empty() {
        return Err(USimpleError::new(1, "not enough arguments"));
    }

    for (name, cfg) in &names {
        search_and_print(name, cfg);
    }

    Ok(())
}

/// Insert a directory into the set, canonicalizing to avoid symlink duplicates.
fn insert_dir(paths: &mut BTreeSet<PathBuf>, path: &Path) {
    if path.is_dir() {
        let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        paths.insert(canonical);
    }
}

fn get_bin_paths(config: &SearchConfig) -> Vec<PathBuf> {
    if let Some(ref custom) = config.custom_bin_paths {
        return custom.clone();
    }

    let mut paths = BTreeSet::new();

    // Hard-coded paths that exist
    for p in BIN_PATHS {
        insert_dir(&mut paths, Path::new(p));
    }

    // Add $PATH entries
    if let Ok(path_var) = std::env::var("PATH") {
        for p in path_var.split(':') {
            insert_dir(&mut paths, Path::new(p));
        }
    }

    paths.into_iter().collect()
}

/// Returns (man_paths, info_paths) separately to preserve output ordering.
fn get_man_and_info_paths(config: &SearchConfig) -> (Vec<PathBuf>, Vec<PathBuf>) {
    if let Some(ref custom) = config.custom_man_paths {
        return (custom.clone(), Vec::new());
    }

    let mut man_paths = BTreeSet::new();
    let mut info_paths = BTreeSet::new();

    // Enumerate subdirectories of man base paths
    for base in MAN_PATHS {
        let base_path = Path::new(base);
        if let Ok(entries) = fs::read_dir(base_path) {
            for entry in entries.flatten() {
                insert_dir(&mut man_paths, &entry.path());
            }
        }
    }

    // Info paths
    for p in INFO_PATHS {
        insert_dir(&mut info_paths, Path::new(p));
    }

    // $MANPATH
    if let Ok(manpath) = std::env::var("MANPATH") {
        for p in manpath.split(':') {
            if p.is_empty() {
                continue;
            }
            let base_path = Path::new(p);
            if let Ok(entries) = fs::read_dir(base_path) {
                for entry in entries.flatten() {
                    insert_dir(&mut man_paths, &entry.path());
                }
            }
            insert_dir(&mut man_paths, base_path);
        }
    }

    (
        man_paths.into_iter().collect(),
        info_paths.into_iter().collect(),
    )
}

fn get_src_paths(config: &SearchConfig) -> Vec<PathBuf> {
    if let Some(ref custom) = config.custom_src_paths {
        return custom.clone();
    }

    let mut paths = BTreeSet::new();

    for base in SRC_PATHS {
        let base_path = Path::new(base);
        if let Ok(entries) = fs::read_dir(base_path) {
            for entry in entries.flatten() {
                insert_dir(&mut paths, &entry.path());
            }
        }
    }

    paths.into_iter().collect()
}

fn print_search_paths(config: &SearchConfig) {
    for p in get_bin_paths(config) {
        println!("bin: {}", p.display());
    }
    let (man_paths, info_paths) = get_man_and_info_paths(config);
    for p in man_paths {
        println!("man: {}", p.display());
    }
    for p in info_paths {
        println!("man: {}", p.display());
    }
    for p in get_src_paths(config) {
        println!("src: {}", p.display());
    }
}

/// Strip leading path components and source-control prefixes from a name.
fn strip_name(name: &str) -> &str {
    let name = match name.rsplit_once('/') {
        Some((_, base)) => base,
        None => name,
    };
    // Strip "s." prefix (SCCS source control)
    name.strip_prefix("s.").unwrap_or(name)
}

/// Check if a filename matches the search name for man/source/info files.
/// Matches:
///   - "name.section[suffix]" where section is a single alnum (e.g., ls.1.gz)
///   - "name.info[suffix]" for info pages (e.g., grep.info.gz)
///   - "name-*" for info pages (e.g., find-maint.info.gz)
/// Does NOT match "python3.14.1.gz" when searching for "python3".
fn name_matches(filename: &str, search_name: &str, use_glob: bool) -> bool {
    if use_glob {
        glob_match(search_name, filename)
    } else {
        if filename == search_name {
            return true;
        }
        if let Some(rest) = filename.strip_prefix(search_name) {
            if let Some(after_dot) = rest.strip_prefix('.') {
                // Man page: ".X" or ".Xp" where X is a single alphanumeric
                if after_dot
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphanumeric())
                    && after_dot.chars().nth(1).map_or(true, |c| c == '.' || c == 'p')
                {
                    return true;
                }
                // Info file: ".info", ".info.gz", ".info-1.gz", etc.
                if after_dot == "info"
                    || after_dot.starts_with("info.")
                    || after_dot.starts_with("info-")
                {
                    return true;
                }
                false
            } else {
                false
            }
        } else {
            false
        }
    }
}

/// Simple glob matching supporting * and ?
fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    glob_match_inner(&pattern, &text)
}

fn glob_match_inner(pattern: &[char], text: &[char]) -> bool {
    if pattern.is_empty() {
        return text.is_empty();
    }
    match pattern[0] {
        '*' => {
            // Try matching * with 0 or more characters
            for i in 0..=text.len() {
                if glob_match_inner(&pattern[1..], &text[i..]) {
                    return true;
                }
            }
            false
        }
        '?' => {
            if text.is_empty() {
                false
            } else {
                glob_match_inner(&pattern[1..], &text[1..])
            }
        }
        c => {
            if text.is_empty() || text[0] != c {
                false
            } else {
                glob_match_inner(&pattern[1..], &text[1..])
            }
        }
    }
}

fn search_in_dirs(name: &str, dirs: &[PathBuf], use_glob: bool, exact: bool) -> Vec<PathBuf> {
    let mut results = Vec::new();

    for dir in dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let file_name_str = file_name.to_string_lossy();
            if exact {
                if use_glob {
                    if glob_match(name, &file_name_str) {
                        results.push(entry.path());
                    }
                } else if file_name_str == name {
                    results.push(entry.path());
                }
            } else if name_matches(&file_name_str, name, use_glob) {
                results.push(entry.path());
            }
        }
    }

    results.sort();
    results.dedup();
    results
}

fn search_and_print(name: &str, config: &SearchConfig) {
    let search_name = strip_name(name);

    let mut bin_results = Vec::new();
    let mut man_results = Vec::new();
    let mut src_results = Vec::new();

    if config.search_bin {
        bin_results = search_in_dirs(search_name, &get_bin_paths(config), config.use_glob, true);
    }
    if config.search_man {
        let (man_dirs, info_dirs) = get_man_and_info_paths(config);
        // Search man pages first, then info - to match util-linux output order
        man_results = search_in_dirs(search_name, &man_dirs, config.use_glob, false);
        let mut info_results = search_in_dirs(search_name, &info_dirs, config.use_glob, false);
        man_results.append(&mut info_results);
    }
    if config.search_src {
        src_results = search_in_dirs(search_name, &get_src_paths(config), config.use_glob, false);
    }

    if config.unusual_only {
        // "unusual" = doesn't have exactly one entry for each requested type
        let mut is_unusual = false;
        if config.search_bin && bin_results.len() != 1 {
            is_unusual = true;
        }
        if config.search_man && man_results.len() != 1 {
            is_unusual = true;
        }
        if config.search_src && src_results.len() != 1 {
            is_unusual = true;
        }
        if !is_unusual {
            return;
        }
    }

    // Output format: "name: path1 path2 ..."
    let display_name = if config.use_glob { name } else { search_name };
    print!("{display_name}:");
    for p in &bin_results {
        print!(" {}", p.display());
    }
    for p in &man_results {
        print!(" {}", p.display());
    }
    for p in &src_results {
        print!(" {}", p.display());
    }
    println!();
}

pub fn uu_app() -> Command {
    Command::new(uucore::util_name())
        .version(crate_version!())
        .about(ABOUT)
        .override_usage(format_usage(USAGE))
        .infer_long_args(true)
        .arg(
            Arg::new("bin")
                .short('b')
                .help("search for binaries")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("man")
                .short('m')
                .help("search for manuals")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("src")
                .short('s')
                .help("search for sources")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("unusual")
                .short('u')
                .help("only show command names with unusual entries")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("glob")
                .short('g')
                .help("interpret names as glob patterns")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("list")
                .short('l')
                .help("output effective lookup paths")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("name")
                .value_name("name")
                .help("command names to search for")
                .action(ArgAction::Set)
                .num_args(1..),
        )
}
