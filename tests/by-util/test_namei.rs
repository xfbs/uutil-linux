// This file is part of the uutils util-linux package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use uutests::{at_and_ucmd, new_ucmd};

#[test]
fn test_invalid_arg() {
    new_ucmd!().arg("--definitely-invalid").fails().code_is(1);
}

#[test]
fn test_missing_pathname() {
    new_ucmd!().fails().code_is(1);
}

#[test]
fn test_basic_path() {
    new_ucmd!()
        .arg("/usr/bin/ls")
        .succeeds()
        .stdout_contains("f: /usr/bin/ls")
        .stdout_contains(" d /")
        .stdout_contains(" d usr")
        .stdout_contains(" d bin")
        .stdout_contains(" - ls");
}

#[test]
fn test_relative_path() {
    let (at, mut ucmd) = at_and_ucmd!();

    at.mkdir("subdir");
    at.touch("subdir/file.txt");

    ucmd.arg("subdir/file.txt")
        .succeeds()
        .stdout_contains("f: subdir/file.txt")
        .stdout_contains(" d subdir")
        .stdout_contains(" - file.txt");
}

#[test]
fn test_symlink() {
    let (at, mut ucmd) = at_and_ucmd!();

    at.touch("target.txt");
    at.symlink_file("target.txt", "link.txt");

    ucmd.arg("link.txt")
        .succeeds()
        .stdout_contains("f: link.txt")
        .stdout_contains(" l link.txt ->")
        .stdout_contains(" - target.txt");
}

#[test]
fn test_nosymlinks() {
    let (at, mut ucmd) = at_and_ucmd!();

    at.touch("target.txt");
    at.symlink_file("target.txt", "link.txt");

    let result = ucmd.args(&["-n", "link.txt"]).succeeds();
    let stdout = result.stdout_str();

    assert!(stdout.contains(" l link.txt ->"));
    // Should NOT follow the symlink - the target filename should not appear
    // on its own line as a resolved component
    let lines: Vec<&str> = stdout.lines().collect();
    // There should only be the "f:" line and the "l" line, no resolved target line
    assert_eq!(lines.len(), 2);
}

#[test]
fn test_modes() {
    new_ucmd!()
        .args(&["-m", "/usr/bin/ls"])
        .succeeds()
        .stdout_contains("rwx");
}

#[test]
fn test_owners() {
    new_ucmd!()
        .args(&["-o", "/usr/bin/ls"])
        .succeeds()
        .stdout_contains("root");
}

#[test]
fn test_long_format() {
    let result = new_ucmd!()
        .args(&["-l", "/usr/bin/ls"])
        .succeeds();
    let stdout = result.stdout_str();

    // Long mode implies modes, owners, and vertical
    assert!(stdout.contains("rwx"));
    assert!(stdout.contains("root"));
}

#[test]
fn test_nonexistent_path() {
    new_ucmd!()
        .arg("/nonexistent/path")
        .fails()
        .code_is(1)
        .stdout_contains("nonexistent - No such file or directory");
}

#[test]
fn test_multiple_paths() {
    let result = new_ucmd!()
        .args(&["/tmp", "/var"])
        .succeeds();
    let stdout = result.stdout_str();

    assert!(stdout.contains("f: /tmp"));
    assert!(stdout.contains("f: /var"));
}

#[test]
fn test_mountpoints() {
    new_ucmd!()
        .args(&["-x", "/"])
        .succeeds()
        .stdout_contains(" D /");
}
