// This file is part of the uutils util-linux package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use clap::{crate_version, Arg, ArgAction, Command};
use std::fs::OpenOptions;
use std::io;
use std::os::fd::AsRawFd;
use uucore::error::{UResult, USimpleError};
use uucore::{format_usage, help_about, help_usage};

const ABOUT: &str = help_about!("delpart.md");
const USAGE: &str = help_usage!("delpart.md");

// BLKPG ioctl number
const BLKPG: libc::c_ulong = 0x1269;
const BLKPG_DEL_PARTITION: i32 = 2;

/// Kernel struct for BLKPG ioctl argument.
#[repr(C)]
struct BlkpgIoctlArg {
    op: i32,
    flags: i32,
    datalen: i32,
    data: *mut BlkpgPartition,
}

/// Kernel struct for partition data.
#[repr(C)]
struct BlkpgPartition {
    start: i64,
    length: i64,
    pno: i32,
    devname: [u8; 64],
    volname: [u8; 64],
}

#[cfg(target_os = "linux")]
#[uucore::main]
pub fn uumain(args: impl uucore::Args) -> UResult<()> {
    let matches = uu_app().try_get_matches_from(args)?;

    let device = matches.get_one::<String>("device").unwrap();
    let partno: i32 = matches
        .get_one::<String>("partition")
        .unwrap()
        .parse()
        .map_err(|_| USimpleError::new(1, "invalid partition number"))?;

    let file = OpenOptions::new()
        .read(true)
        .open(device)
        .map_err(|e| USimpleError::new(1, format!("cannot open {device}: {e}")))?;

    // For delete, only the partition number matters
    let mut part = BlkpgPartition {
        start: 0,
        length: 0,
        pno: partno,
        devname: [0; 64],
        volname: [0; 64],
    };

    let arg = BlkpgIoctlArg {
        op: BLKPG_DEL_PARTITION,
        flags: 0,
        datalen: std::mem::size_of::<BlkpgPartition>() as i32,
        data: &mut part,
    };

    let ret = unsafe { libc::ioctl(file.as_raw_fd(), BLKPG, &arg) };
    if ret < 0 {
        let err = io::Error::last_os_error();
        return Err(USimpleError::new(
            1,
            format!("failed to remove partition: {err}"),
        ));
    }

    Ok(())
}

#[cfg(not(target_os = "linux"))]
#[uucore::main]
pub fn uumain(args: impl uucore::Args) -> UResult<()> {
    let _matches = uu_app().try_get_matches_from(args)?;
    Err(USimpleError::new(1, "`delpart` is available only on Linux."))
}

pub fn uu_app() -> Command {
    Command::new(uucore::util_name())
        .version(crate_version!())
        .about(ABOUT)
        .override_usage(format_usage(USAGE))
        .infer_long_args(true)
        .arg(
            Arg::new("device")
                .help("disk device")
                .required(true)
                .index(1)
                .action(ArgAction::Set),
        )
        .arg(
            Arg::new("partition")
                .help("partition number")
                .required(true)
                .index(2)
                .action(ArgAction::Set),
        )
}
