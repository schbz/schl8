use anyhow::Result;

/// Lock down the process: disable core dumps and check mlock limits.
/// Must be called before any sensitive data is loaded into memory.
pub fn lock_down() -> Result<()> {
    disable_core_dumps()?;
    check_mlock_limits();
    Ok(())
}

/// Disable core dumps to prevent sensitive data from being written to disk.
fn disable_core_dumps() -> Result<()> {
    let zero = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    let ret = unsafe { libc::setrlimit(libc::RLIMIT_CORE, &zero) };
    if ret != 0 {
        anyhow::bail!(
            "failed to disable core dumps: {}",
            std::io::Error::last_os_error()
        );
    }
    Ok(())
}

/// Warn if mlock limits are too low for typical document sizes.
fn check_mlock_limits() {
    let mut rlim = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    let ret = unsafe { libc::getrlimit(libc::RLIMIT_MEMLOCK, &mut rlim) };
    if ret == 0 && rlim.rlim_cur < 64 * 1024 {
        eprintln!(
            "warning: mlock limit is only {} bytes — large documents may not be fully protected from swap",
            rlim.rlim_cur
        );
    }
}

/// Lock a memory region to prevent it from being swapped to disk.
///
/// # Safety
/// `ptr` must point to a valid allocation of at least `len` bytes.
pub unsafe fn mlock(ptr: *const u8, len: usize) -> bool {
    if len == 0 {
        return true;
    }
    unsafe { libc::mlock(ptr as *const libc::c_void, len) == 0 }
}

/// Unlock a previously locked memory region.
///
/// # Safety
/// `ptr` must point to a region previously locked with `mlock`.
pub unsafe fn munlock(ptr: *const u8, len: usize) -> bool {
    if len == 0 {
        return true;
    }
    unsafe { libc::munlock(ptr as *const libc::c_void, len) == 0 }
}
