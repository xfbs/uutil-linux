// This file is part of the uutils util-linux package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use uutests::new_ucmd;

#[test]
fn test_no_args() {
    new_ucmd!().fails().code_is(1);
}

#[test]
fn test_basic_lookup() {
    let result = new_ucmd!().arg("ls").succeeds();
    let stdout = result.stdout_str();

    assert!(stdout.starts_with("ls:"));
    assert!(stdout.contains("/usr/bin/ls") || stdout.contains("/bin/ls"));
}

#[test]
fn test_binary_only() {
    let result = new_ucmd!().args(&["-b", "ls"]).succeeds();
    let stdout = result.stdout_str();

    assert!(stdout.starts_with("ls:"));
    assert!(stdout.contains("/bin/ls") || stdout.contains("/usr/bin/ls"));
    // Should not contain man page paths
    assert!(!stdout.contains(".1.gz"));
}

#[test]
fn test_manual_only() {
    let result = new_ucmd!().args(&["-m", "ls"]).succeeds();
    let stdout = result.stdout_str();

    assert!(stdout.starts_with("ls:"));
    assert!(stdout.contains("man"));
}

#[test]
fn test_source_only() {
    let result = new_ucmd!().args(&["-s", "ls"]).succeeds();
    let stdout = result.stdout_str();

    // ls probably has no source, so just "ls:" with nothing after
    assert!(stdout.starts_with("ls:"));
}

#[test]
fn test_nonexistent() {
    let result = new_ucmd!().arg("xyzzy_totally_nonexistent").succeeds();
    let stdout = result.stdout_str();

    assert_eq!(stdout.trim(), "xyzzy_totally_nonexistent:");
}

#[test]
fn test_multiple_names() {
    let result = new_ucmd!().args(&["ls", "cat"]).succeeds();
    let stdout = result.stdout_str();

    assert!(stdout.contains("ls:"));
    assert!(stdout.contains("cat:"));
}

#[test]
fn test_list_paths() {
    let result = new_ucmd!().args(&["-l"]).succeeds();
    let stdout = result.stdout_str();

    assert!(stdout.contains("bin:"));
    assert!(stdout.contains("man:"));
}

#[test]
fn test_custom_bin_path() {
    let result = new_ucmd!()
        .args(&["-B", "/usr/bin", "-f", "ls"])
        .succeeds();
    let stdout = result.stdout_str();

    assert!(stdout.starts_with("ls:"));
    assert!(stdout.contains("/usr/bin/ls"));
}

#[test]
fn test_combined_flags() {
    let result = new_ucmd!().args(&["-bm", "ls"]).succeeds();
    let stdout = result.stdout_str();

    assert!(stdout.starts_with("ls:"));
    // Should have both binary and man page
    assert!(stdout.contains("/bin/ls") || stdout.contains("/usr/bin/ls"));
    assert!(stdout.contains("man"));
}
