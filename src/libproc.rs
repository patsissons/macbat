//! Hand-written libproc FFI: process listing, names, paths and resource usage.
//!
//! libproc lives in libSystem, so no `#[link]` is needed. Everything here works without root,
//! but `proc_pid_rusage` only succeeds for processes owned by the calling user.

use std::ffi::{c_int, c_void};

const RUSAGE_INFO_V6: c_int = 6;
/// PROC_PIDPATHINFO_MAXSIZE (4 × MAXPATHLEN).
const PATH_MAX: usize = 4096;

/// `struct rusage_info_v6` from `sys/resource.h`. Only the two fields macbat reads are named;
/// the rest is padding, kept at full size because the kernel writes the whole struct.
#[repr(C)]
pub struct RusageInfoV6 {
    uuid: [u8; 16],
    head: [u64; 8],
    /// Process start, in mach absolute time. With the pid it identifies a process uniquely.
    pub proc_start_abstime: u64,
    mid: [u64; 31],
    /// Cumulative CPU energy billed to the process, nJ.
    pub energy_nj: u64,
    tail: [u64; 15],
}

const _: () = assert!(std::mem::size_of::<RusageInfoV6>() == 464);

unsafe extern "C" {
    fn proc_listallpids(buffer: *mut c_void, buffersize: c_int) -> c_int;
    fn proc_pid_rusage(pid: c_int, flavor: c_int, buffer: *mut c_void) -> c_int;
    fn proc_name(pid: c_int, buffer: *mut c_void, buffersize: u32) -> c_int;
    fn proc_pidpath(pid: c_int, buffer: *mut c_void, buffersize: u32) -> c_int;
}

/// Every pid on the system, including ones this user can't inspect.
pub fn pids() -> Vec<i32> {
    let count = unsafe { proc_listallpids(std::ptr::null_mut(), 0) }.max(0) as usize;
    // Headroom for processes started between the two calls.
    let mut buf = vec![0i32; count + 64];
    let bytes = (buf.len() * std::mem::size_of::<i32>()) as c_int;
    let n = unsafe { proc_listallpids(buf.as_mut_ptr() as _, bytes) }.max(0) as usize;
    buf.truncate(n.min(buf.len()));
    buf
}

/// Resource usage for `pid`, or `None` when it has exited or belongs to another user (EPERM).
pub fn rusage(pid: i32) -> Option<RusageInfoV6> {
    let mut info = std::mem::MaybeUninit::<RusageInfoV6>::zeroed();
    let ok = unsafe { proc_pid_rusage(pid, RUSAGE_INFO_V6, info.as_mut_ptr() as _) } == 0;
    ok.then(|| unsafe { info.assume_init() })
}

/// The process name (at most 31 bytes), empty when unavailable.
pub fn name(pid: i32) -> String {
    let mut buf = [0u8; 64];
    let n = unsafe { proc_name(pid, buf.as_mut_ptr() as _, buf.len() as u32) }.max(0) as usize;
    String::from_utf8_lossy(&buf[..n.min(buf.len())]).into_owned()
}

/// The executable's full path.
pub fn path(pid: i32) -> Option<String> {
    let mut buf = vec![0u8; PATH_MAX];
    let n = unsafe { proc_pidpath(pid, buf.as_mut_ptr() as _, buf.len() as u32) };
    (n > 0).then(|| String::from_utf8_lossy(&buf[..(n as usize).min(PATH_MAX)]).into_owned())
}
