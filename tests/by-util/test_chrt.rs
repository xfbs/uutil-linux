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
fn test_show_max_priorities() {
    let result = new_ucmd!().arg("-m").succeeds();
    let stdout = result.stdout_str();

    assert!(stdout.contains("SCHED_OTHER"));
    assert!(stdout.contains("SCHED_FIFO"));
    assert!(stdout.contains("SCHED_RR"));
    assert!(stdout.contains("SCHED_BATCH"));
    assert!(stdout.contains("SCHED_IDLE"));
    assert!(stdout.contains("SCHED_DEADLINE"));
}

#[test]
fn test_query_pid() {
    let pid = std::process::id().to_string();
    let result = new_ucmd!().args(&["-p", &pid]).succeeds();
    let stdout = result.stdout_str();

    assert!(stdout.contains("scheduling policy:"));
    assert!(stdout.contains("scheduling priority:"));
}

#[test]
fn test_set_other_exec() {
    new_ucmd!()
        .args(&["-o", "0", "echo", "hello"])
        .succeeds()
        .stdout_contains("hello");
}

#[test]
fn test_set_batch_exec() {
    new_ucmd!()
        .args(&["-b", "0", "echo", "batch"])
        .succeeds()
        .stdout_contains("batch");
}

#[test]
fn test_set_idle_exec() {
    new_ucmd!()
        .args(&["-i", "0", "echo", "idle"])
        .succeeds()
        .stdout_contains("idle");
}

#[test]
fn test_set_pid_other() {
    let pid = std::process::id().to_string();
    new_ucmd!()
        .args(&["-o", "-p", "0", &pid])
        .succeeds();

    // Verify the policy was set
    new_ucmd!()
        .args(&["-p", &pid])
        .succeeds()
        .stdout_contains("SCHED_OTHER");
}

#[test]
fn test_nonexistent_command() {
    new_ucmd!()
        .args(&["-o", "0", "nonexistent_command_xyz"])
        .fails()
        .code_is(127);
}
