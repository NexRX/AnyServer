//! Supplementary process hardening — always available on Linux.
//! Called inside `pre_exec` hooks; uses only raw `libc` calls.

/// Mark all FDs beyond 0/1/2 as close-on-exec. Uses `CLOEXEC` rather than
/// `close()` so Rust's internal exec-error pipe survives until `execve`.
pub fn close_inherited_fds() {
    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
    {
        const SYS_CLOSE_RANGE: libc::c_long = 436;
        const CLOSE_RANGE_CLOEXEC: libc::c_uint = 4;
        let ret = unsafe { libc::syscall(SYS_CLOSE_RANGE, 3u32, u32::MAX, CLOSE_RANGE_CLOEXEC) };
        if ret == 0 {
            return;
        }
    }

    let max_fd = {
        let mut rlim: libc::rlimit = unsafe { std::mem::zeroed() };
        let ret = unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut rlim) };
        if ret == 0 && rlim.rlim_cur > 0 {
            std::cmp::min(rlim.rlim_cur as i32, 4096)
        } else {
            1024
        }
    };

    for fd in 3..max_fd {
        unsafe {
            libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
        }
    }
}

/// Irreversibly prevent privilege escalation via suid/sgid/caps.
/// Also a prerequisite for `landlock_restrict_self()`.
pub fn apply_no_new_privs() {
    unsafe {
        libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0);
    }
}

/// Prevent ptrace attachment and `/proc/<pid>/mem` reads.
pub fn set_non_dumpable() {
    unsafe {
        libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0);
    }
}

/// Fork-bomb protection. **Caution:** `RLIMIT_NPROC` is per-UID, not
/// per-process — a value that's too low affects AnyServer and siblings.
pub fn set_nproc_limit(limit: u64) {
    let rlim = libc::rlimit {
        rlim_cur: limit as libc::rlim_t,
        rlim_max: limit as libc::rlim_t,
    };
    unsafe {
        libc::setrlimit(libc::RLIMIT_NPROC, &rlim);
    }
}
