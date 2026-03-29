//! PID and mount namespace isolation for RunCommand pipeline steps.
//! Network namespaces are intentionally not used — steps often need network access.

use std::io;

pub fn apply_namespaces() -> io::Result<()> {
    #[cfg(target_os = "linux")]
    {
        let flags = libc::CLONE_NEWPID | libc::CLONE_NEWNS;

        let result = unsafe { libc::unshare(flags) };

        if result != 0 {
            return Err(io::Error::last_os_error());
        }

        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Namespaces are only supported on Linux",
        ))
    }
}

pub fn probe_namespace_support() -> bool {
    #[cfg(target_os = "linux")]
    {
        match unsafe { libc::fork() } {
            -1 => false,
            0 => {
                let flags = libc::CLONE_NEWPID | libc::CLONE_NEWNS;
                let result = unsafe { libc::unshare(flags) };
                unsafe { libc::_exit(if result == 0 { 0 } else { 1 }) };
            }
            child_pid => {
                let mut status = 0;
                unsafe { libc::waitpid(child_pid, &mut status, 0) };
                libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_probe_namespace_support() {
        let _ = probe_namespace_support();
    }
}
