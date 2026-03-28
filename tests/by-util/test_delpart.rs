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
fn test_missing_args() {
    new_ucmd!().fails().code_is(1);
}

#[test]
fn test_not_a_block_device() {
    new_ucmd!()
        .args(&["/dev/null", "1"])
        .fails()
        .code_is(1)
        .stderr_contains("failed to remove partition");
}

#[test]
fn test_invalid_partition_number() {
    new_ucmd!()
        .args(&["/dev/null", "abc"])
        .fails()
        .code_is(1)
        .stderr_contains("invalid partition number");
}
