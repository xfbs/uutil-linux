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
fn test_basic_exclusive_lock() {
    let (at, mut ucmd) = at_and_ucmd!();

    ucmd.args(&[at.plus_as_string("lockfile").as_str(), "echo", "hello"])
        .succeeds()
        .stdout_contains("hello");
}

#[test]
fn test_shared_lock() {
    let (at, mut ucmd) = at_and_ucmd!();

    ucmd.args(&["-s", at.plus_as_string("lockfile").as_str(), "echo", "shared"])
        .succeeds()
        .stdout_contains("shared");
}

#[test]
fn test_shell_command() {
    let (at, mut ucmd) = at_and_ucmd!();

    ucmd.args(&[
        at.plus_as_string("lockfile").as_str(),
        "-c",
        "echo hello from shell",
    ])
    .succeeds()
    .stdout_contains("hello from shell");
}

#[test]
fn test_exit_code_passthrough() {
    let (at, mut ucmd) = at_and_ucmd!();

    ucmd.args(&[
        at.plus_as_string("lockfile").as_str(),
        "bash",
        "-c",
        "exit 42",
    ])
    .fails()
    .code_is(42);
}

#[test]
fn test_verbose() {
    let (at, mut ucmd) = at_and_ucmd!();

    ucmd.args(&[
        "--verbose",
        at.plus_as_string("lockfile").as_str(),
        "echo",
        "test",
    ])
    .succeeds()
    .stderr_contains("getting lock took")
    .stderr_contains("executing echo");
}

#[test]
fn test_creates_lock_file() {
    let (at, mut ucmd) = at_and_ucmd!();

    let lockfile = at.plus_as_string("newlockfile");
    assert!(!at.file_exists("newlockfile"));

    ucmd.args(&[lockfile.as_str(), "echo", "created"])
        .succeeds()
        .stdout_contains("created");

    assert!(at.file_exists("newlockfile"));
}

#[test]
fn test_no_fork_close_incompatible() {
    let (at, mut ucmd) = at_and_ucmd!();

    ucmd.args(&[
        "-F",
        "-o",
        at.plus_as_string("lockfile").as_str(),
        "echo",
        "bad",
    ])
    .fails()
    .code_is(1)
    .stderr_contains("incompatible");
}

#[test]
fn test_nonblock_no_command() {
    let (at, mut ucmd) = at_and_ucmd!();

    // Nonblock on an unlocked file should succeed
    ucmd.args(&["-n", at.plus_as_string("lockfile").as_str(), "echo", "got it"])
        .succeeds()
        .stdout_contains("got it");
}
