// This file is part of the uutils util-linux package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use uutests::new_ucmd;

#[test]
fn test_invalid_arg() {
    new_ucmd!().arg("--definitely-invalid").fails().code_is(1);
}

#[test]
fn test_query_self() {
    new_ucmd!()
        .succeeds()
        .stdout_contains(": prio ");
}

#[test]
fn test_query_pid() {
    let pid = std::process::id().to_string();
    new_ucmd!()
        .args(&["-p", &pid])
        .succeeds()
        .stdout_contains(": prio ");
}

#[test]
fn test_set_and_exec() {
    new_ucmd!()
        .args(&["-c", "2", "-n", "4", "echo", "hello"])
        .succeeds()
        .stdout_contains("hello");
}

#[test]
fn test_class_name_best_effort() {
    new_ucmd!()
        .args(&["-c", "best-effort", "-n", "0", "echo", "test"])
        .succeeds()
        .stdout_contains("test");
}

#[test]
fn test_class_name_idle() {
    new_ucmd!()
        .args(&["-c", "idle", "echo", "idle"])
        .succeeds()
        .stdout_contains("idle");
}

#[test]
fn test_set_pid() {
    let pid = std::process::id().to_string();
    new_ucmd!()
        .args(&["-c", "2", "-n", "5", "-p", &pid])
        .succeeds();

    // Verify it was set
    new_ucmd!()
        .args(&["-p", &pid])
        .succeeds()
        .stdout_contains("best-effort: prio 5");
}

#[test]
fn test_ignore_flag() {
    // -t should not fail even if setting priority fails
    // (e.g., trying realtime as non-root)
    new_ucmd!()
        .args(&["-c", "1", "-n", "0", "-t", "echo", "test"])
        .succeeds()
        .stdout_contains("test");
}

#[test]
fn test_nonexistent_command() {
    new_ucmd!()
        .args(&["-c", "2", "nonexistent_command_xyz"])
        .fails()
        .code_is(127);
}
