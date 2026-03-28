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
fn test_no_mountpoint() {
    new_ucmd!().fails().code_is(1);
}

#[test]
fn test_dry_run_verbose() {
    // Dry run on root should succeed and show output
    new_ucmd!()
        .args(&["-v", "-n", "/"])
        .succeeds()
        .stdout_contains("/: 0 B (dry run) trimmed");
}

#[test]
fn test_dry_run_all() {
    // Dry run with --all should succeed
    new_ucmd!().args(&["-n", "-a"]).succeeds();
}

#[test]
fn test_dry_run_fstab() {
    // Dry run with --fstab should succeed
    new_ucmd!().args(&["-n", "-A"]).succeeds();
}

#[test]
fn test_not_a_directory() {
    new_ucmd!()
        .args(&["-n", "/etc/fstab"])
        .fails()
        .code_is(1);
}

#[test]
fn test_nonexistent_mountpoint() {
    new_ucmd!()
        .args(&["-n", "/nonexistent_mountpoint_xyz"])
        .fails()
        .code_is(1);
}
