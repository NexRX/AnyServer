//! Landlock filesystem sandboxing — Linux 5.13+ (ABI v1 minimum).
//! Uses raw `libc::syscall()` for async-signal-safety in `pre_exec` hooks.

use std::ffi::CString;
use std::io;

#[cfg(target_arch = "x86_64")]
const SYS_LANDLOCK_CREATE_RULESET: libc::c_long = 444;
#[cfg(target_arch = "x86_64")]
const SYS_LANDLOCK_ADD_RULE: libc::c_long = 445;
#[cfg(target_arch = "x86_64")]
const SYS_LANDLOCK_RESTRICT_SELF: libc::c_long = 446;

#[cfg(target_arch = "aarch64")]
const SYS_LANDLOCK_CREATE_RULESET: libc::c_long = 444;
#[cfg(target_arch = "aarch64")]
const SYS_LANDLOCK_ADD_RULE: libc::c_long = 445;
#[cfg(target_arch = "aarch64")]
const SYS_LANDLOCK_RESTRICT_SELF: libc::c_long = 446;

const LANDLOCK_CREATE_RULESET_VERSION: u32 = 1 << 0;
const LANDLOCK_RULE_PATH_BENEATH: u32 = 1;

const LANDLOCK_ACCESS_FS_EXECUTE: u64 = 1 << 0;
const LANDLOCK_ACCESS_FS_WRITE_FILE: u64 = 1 << 1;
const LANDLOCK_ACCESS_FS_READ_FILE: u64 = 1 << 2;
const LANDLOCK_ACCESS_FS_READ_DIR: u64 = 1 << 3;
const LANDLOCK_ACCESS_FS_REMOVE_DIR: u64 = 1 << 4;
const LANDLOCK_ACCESS_FS_REMOVE_FILE: u64 = 1 << 5;
const LANDLOCK_ACCESS_FS_MAKE_CHAR: u64 = 1 << 6;
const LANDLOCK_ACCESS_FS_MAKE_DIR: u64 = 1 << 7;
const LANDLOCK_ACCESS_FS_MAKE_REG: u64 = 1 << 8;
const LANDLOCK_ACCESS_FS_MAKE_SOCK: u64 = 1 << 9;
const LANDLOCK_ACCESS_FS_MAKE_FIFO: u64 = 1 << 10;
const LANDLOCK_ACCESS_FS_MAKE_BLOCK: u64 = 1 << 11;
const LANDLOCK_ACCESS_FS_MAKE_SYM: u64 = 1 << 12;
const LANDLOCK_ACCESS_FS_REFER: u64 = 1 << 13; // ABI v2
const LANDLOCK_ACCESS_FS_TRUNCATE: u64 = 1 << 14; // ABI v3
const LANDLOCK_ACCESS_FS_IOCTL_DEV: u64 = 1 << 15; // ABI v4

const ALL_FS_V1: u64 = LANDLOCK_ACCESS_FS_EXECUTE
    | LANDLOCK_ACCESS_FS_WRITE_FILE
    | LANDLOCK_ACCESS_FS_READ_FILE
    | LANDLOCK_ACCESS_FS_READ_DIR
    | LANDLOCK_ACCESS_FS_REMOVE_DIR
    | LANDLOCK_ACCESS_FS_REMOVE_FILE
    | LANDLOCK_ACCESS_FS_MAKE_CHAR
    | LANDLOCK_ACCESS_FS_MAKE_DIR
    | LANDLOCK_ACCESS_FS_MAKE_REG
    | LANDLOCK_ACCESS_FS_MAKE_SOCK
    | LANDLOCK_ACCESS_FS_MAKE_FIFO
    | LANDLOCK_ACCESS_FS_MAKE_BLOCK
    | LANDLOCK_ACCESS_FS_MAKE_SYM;

const FS_READ_ONLY: u64 =
    LANDLOCK_ACCESS_FS_EXECUTE | LANDLOCK_ACCESS_FS_READ_FILE | LANDLOCK_ACCESS_FS_READ_DIR;

fn fs_full_access(abi: i32) -> u64 {
    let mut mask = ALL_FS_V1;
    if abi >= 2 {
        mask |= LANDLOCK_ACCESS_FS_REFER;
    }
    if abi >= 3 {
        mask |= LANDLOCK_ACCESS_FS_TRUNCATE;
    }
    if abi >= 4 {
        mask |= LANDLOCK_ACCESS_FS_IOCTL_DEV;
    }
    mask
}

/// Rights valid for non-directory files only. The kernel rejects rules
/// with directory-only rights (READ_DIR, MAKE_*, etc.) on file inodes.
fn fs_file_access(abi: i32) -> u64 {
    let mut mask =
        LANDLOCK_ACCESS_FS_EXECUTE | LANDLOCK_ACCESS_FS_WRITE_FILE | LANDLOCK_ACCESS_FS_READ_FILE;
    if abi >= 3 {
        mask |= LANDLOCK_ACCESS_FS_TRUNCATE;
    }
    if abi >= 4 {
        mask |= LANDLOCK_ACCESS_FS_IOCTL_DEV;
    }
    mask
}

fn handled_access_mask(abi: i32) -> u64 {
    fs_full_access(abi)
}

#[repr(C)]
struct RulesetAttr {
    handled_access_fs: u64,
    handled_access_net: u64,
}

#[repr(C)]
struct PathBeneathAttr {
    allowed_access: u64,
    parent_fd: i32,
}

pub fn probe_abi_version() -> Option<i32> {
    let ret = unsafe {
        libc::syscall(
            SYS_LANDLOCK_CREATE_RULESET,
            std::ptr::null::<RulesetAttr>(),
            0usize,
            LANDLOCK_CREATE_RULESET_VERSION,
        )
    };
    if ret > 0 {
        Some(ret as i32)
    } else {
        None
    }
}

pub fn apply_landlock(rw_paths: &[CString], ro_paths: &[CString]) -> io::Result<()> {
    let abi = probe_abi_version().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::Unsupported,
            "Landlock not supported by this kernel",
        )
    })?;

    let handled = handled_access_mask(abi);
    let full_rw = fs_full_access(abi);
    let ro_mask = FS_READ_ONLY & handled;

    let attr = RulesetAttr {
        handled_access_fs: handled,
        handled_access_net: 0,
    };

    let attr_size = if abi >= 4 {
        std::mem::size_of::<RulesetAttr>()
    } else {
        std::mem::size_of::<u64>() // just the first field
    };

    let ruleset_fd = unsafe {
        libc::syscall(
            SYS_LANDLOCK_CREATE_RULESET,
            &attr as *const RulesetAttr,
            attr_size,
            0u32,
        )
    };
    if ruleset_fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let ruleset_fd = ruleset_fd as i32;

    let file_rw = fs_file_access(abi);

    let add_path_rule = |path: &CString, access: u64| {
        let fd = unsafe { libc::open(path.as_ptr(), libc::O_PATH | libc::O_CLOEXEC) };
        if fd < 0 {
            return;
        }

        let effective_access = {
            let mut st: libc::stat = unsafe { std::mem::zeroed() };
            let is_dir = unsafe { libc::fstat(fd, &mut st) } == 0
                && (st.st_mode & libc::S_IFMT) == libc::S_IFDIR;
            if is_dir {
                access
            } else {
                access & file_rw
            }
        };

        let rule = PathBeneathAttr {
            allowed_access: effective_access,
            parent_fd: fd,
        };

        let ret = unsafe {
            libc::syscall(
                SYS_LANDLOCK_ADD_RULE,
                ruleset_fd,
                LANDLOCK_RULE_PATH_BENEATH,
                &rule as *const PathBeneathAttr,
                0u32,
            )
        };

        if ret < 0 {
            let err = unsafe { *libc::__errno_location() };
            let msg = format!(
                "[anyserver sandbox] landlock_add_rule failed for {:?}: errno {}\n",
                path, err,
            );
            unsafe {
                libc::write(2, msg.as_ptr() as *const libc::c_void, msg.len());
            }
        }

        unsafe {
            libc::close(fd);
        }
    };

    for path in rw_paths {
        add_path_rule(path, full_rw);
    }

    for path in ro_paths {
        add_path_rule(path, ro_mask);
    }

    let nnp_ret = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };
    if nnp_ret != 0 {
        unsafe { libc::close(ruleset_fd) };
        return Err(io::Error::last_os_error());
    }

    let ret = unsafe { libc::syscall(SYS_LANDLOCK_RESTRICT_SELF, ruleset_fd, 0u32) };

    unsafe { libc::close(ruleset_fd) };

    if ret < 0 {
        return Err(io::Error::last_os_error());
    }

    Ok(())
}
