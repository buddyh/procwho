//! Thin, read-only wrappers over libproc, sysctl and a few libc time helpers.
//!
//! Every function here only reads kernel state. Nothing signals, stops or
//! modifies a process, with one exception: [`run`] kills its OWN helper subprocess
//! group when that helper times out.

use std::ffi::c_void;
use std::io::Read;
use std::os::raw::{c_char, c_int};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[repr(C)]
struct MachTimebase {
    numer: u32,
    denom: u32,
}

extern "C" {
    fn mach_host_self() -> u32;
    fn mach_port_deallocate(task: u32, name: u32) -> c_int;
    fn host_page_size(host: u32, size: *mut u32) -> c_int;
    static mach_task_self_: u32;
    fn proc_listallpids(buffer: *mut c_void, buffersize: c_int) -> c_int;
    fn proc_pidinfo(
        pid: c_int,
        flavor: c_int,
        arg: u64,
        buffer: *mut c_void,
        buffersize: c_int,
    ) -> c_int;
    fn proc_pidpath(pid: c_int, buffer: *mut c_void, buffersize: u32) -> c_int;
    fn proc_pid_rusage(pid: c_int, flavor: c_int, buffer: *mut c_void) -> c_int;
    fn mach_timebase_info(info: *mut MachTimebase) -> c_int;
}

const PROC_PIDTBSDINFO: c_int = 3;
const PROC_PIDVNODEPATHINFO: c_int = 9;
const PROC_PIDT_SHORTBSDINFO: c_int = 13;
const PROC_PIDUNIQIDENTIFIERINFO: c_int = 17;
const RUSAGE_INFO_V4: c_int = 4;
// Darwin's LP64 ABI uses fixed-width fields here on both x86_64 and arm64.
// Checked against <sys/proc_info.h> for both targets; reject short/changed layouts below.
const VNODE_INFO_LEN: usize = 152; // sizeof(struct vnode_info) before the path
const VNODE_PATH_LEN: usize = 1024;
const VNODE_PATHINFO_SIZE: usize = 2 * (VNODE_INFO_LEN + VNODE_PATH_LEN);

/// Environment keys that may ever be copied out of a process. Nothing else is kept.
pub const ENV_ALLOW: [&str; 4] = [
    "T3CODE_THREAD_ID",
    "CLAUDE_CODE_ENTRYPOINT",
    "TMUX_PANE",
    "PROCWHO_OWNER",
];

/// `struct proc_bsdinfo` (PROC_PIDTBSDINFO). Same uid only.
#[repr(C)]
#[derive(Clone, Copy)]
struct BsdInfo {
    flags: u32,
    status: u32,
    xstatus: u32,
    pid: u32,
    ppid: u32,
    uid: u32,
    gid: u32,
    ruid: u32,
    rgid: u32,
    svuid: u32,
    svgid: u32,
    rfu1: u32,
    comm: [u8; 16],
    name: [u8; 32],
    nfiles: u32,
    pgid: u32,
    pjobc: u32,
    tdev: u32,
    tpgid: u32,
    nice: i32,
    start_sec: u64,
    start_usec: u64,
}

/// `struct proc_bsdshortinfo` (PROC_PIDT_SHORTBSDINFO). Any uid.
#[repr(C)]
#[derive(Clone, Copy)]
struct ShortBsdInfo {
    pid: u32,
    ppid: u32,
    pgid: u32,
    status: u32,
    comm: [u8; 16],
    flags: u32,
    uid: u32,
    gid: u32,
    ruid: u32,
    rgid: u32,
    svuid: u32,
    svgid: u32,
    rfu: u32,
}

/// `struct proc_uniqidentifierinfo` (PROC_PIDUNIQIDENTIFIERINFO). Any uid.
#[repr(C)]
#[derive(Clone, Copy)]
struct UniqInfo {
    uuid: [u8; 16],
    uniqueid: u64,
    puniqueid: u64,
    idversion: i32,
    reserve2: u32,
    reserve3: u64,
    reserve4: u64,
}

/// Basic identity of a process, readable for any uid.
#[derive(Debug, Clone, Default)]
pub struct Basic {
    pub ppid: i32,
    pub pgid: i32,
    pub uid: u32,
    pub comm: String,
    pub zombie: bool,
    pub tty_dev: u32,
    pub start: f64,
}

/// Memory and CPU counters from `proc_pid_rusage` v4.
#[derive(Debug, Clone, Copy, Default)]
pub struct Usage {
    pub footprint: u64,
    pub resident: u64,
    pub cpu_ticks: u64,
    pub life_max_footprint: u64,
}

fn ticks_to_ns(ticks: u64, numer: u32, denom: u32) -> u64 {
    if denom == 0 {
        return ticks;
    }
    (u128::from(ticks) * u128::from(numer) / u128::from(denom)).min(u128::from(u64::MAX)) as u64
}

fn cstr(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).into_owned()
}

/// Reads process data through libproc and sysctl with reusable buffers.
pub struct Reader {
    args: Vec<u8>,
    path: Vec<u8>,
    vnode: Vec<u8>,
    tick_num: u32,
    tick_den: u32,
    responsible: Option<extern "C" fn(c_int) -> c_int>,
}

impl Default for Reader {
    fn default() -> Self {
        Self::new()
    }
}

impl Reader {
    pub fn new() -> Self {
        let mut tb = MachTimebase { numer: 1, denom: 1 };
        // SAFETY: tb is a valid out pointer for the duration of the call.
        let result = unsafe { mach_timebase_info(&mut tb) };
        if result != 0 || tb.numer == 0 || tb.denom == 0 {
            tb = MachTimebase { numer: 1, denom: 1 };
        }
        Reader {
            args: vec![0u8; argmax()],
            path: vec![0u8; 4096],
            vnode: vec![0u8; VNODE_PATHINFO_SIZE],
            tick_num: tb.numer,
            tick_den: tb.denom,
            responsible: lookup_responsible(),
        }
    }

    /// Mach ticks to nanoseconds.
    pub fn ticks_to_ns(&self, ticks: u64) -> u64 {
        ticks_to_ns(ticks, self.tick_num, self.tick_den)
    }

    /// All live PIDs (pid 0 excluded).
    pub fn all_pids(&self) -> Vec<i32> {
        // SAFETY: a null buffer asks for the count only.
        let n = unsafe { proc_listallpids(std::ptr::null_mut(), 0) };
        if n <= 0 {
            return Vec::new();
        }
        let cap = n as usize + 256;
        let mut v = vec![0i32; cap];
        // SAFETY: v has cap * 4 bytes of writable storage.
        let got = unsafe { proc_listallpids(v.as_mut_ptr() as *mut c_void, (cap * 4) as c_int) };
        v.truncate((got.max(0) as usize).min(cap));
        v.retain(|&p| p > 0);
        v
    }

    fn info<T: Copy>(pid: i32, flavor: c_int) -> Option<T> {
        // SAFETY: T is a plain repr(C) struct of integers and byte arrays; all-zero is valid.
        let mut out: T = unsafe { std::mem::zeroed() };
        let sz = std::mem::size_of::<T>() as c_int;
        // SAFETY: out is writable for sz bytes.
        let r = unsafe { proc_pidinfo(pid, flavor, 0, &mut out as *mut T as *mut c_void, sz) };
        (r == sz).then_some(out)
    }

    /// (unique id, original parent's unique id). Readable for any uid.
    pub fn uniq(&self, pid: i32) -> Option<(u64, u64)> {
        Self::info::<UniqInfo>(pid, PROC_PIDUNIQIDENTIFIERINFO).map(|u| (u.uniqueid, u.puniqueid))
    }

    pub fn exe_path(&mut self, pid: i32) -> String {
        // SAFETY: path buffer is 4096 bytes.
        let n = unsafe {
            proc_pidpath(
                pid,
                self.path.as_mut_ptr() as *mut c_void,
                self.path.len() as u32,
            )
        };
        if n > 0 {
            String::from_utf8_lossy(&self.path[..n as usize]).into_owned()
        } else {
            String::new()
        }
    }

    /// Full BSD info (same uid) or the short form (any uid). None if the PID is gone.
    /// The bool is true when the full form was readable.
    pub fn basic(&self, pid: i32) -> Option<(Basic, bool)> {
        if let Some(b) = Self::info::<BsdInfo>(pid, PROC_PIDTBSDINFO) {
            return Some((
                Basic {
                    ppid: b.ppid as i32,
                    pgid: b.pgid as i32,
                    uid: b.uid,
                    comm: cstr(&b.comm),
                    zombie: b.status == 5,
                    tty_dev: b.tdev,
                    start: b.start_sec as f64 + b.start_usec as f64 / 1e6,
                },
                true,
            ));
        }
        let s = Self::info::<ShortBsdInfo>(pid, PROC_PIDT_SHORTBSDINFO)?;
        Some((
            Basic {
                ppid: s.ppid as i32,
                pgid: s.pgid as i32,
                uid: s.uid,
                comm: cstr(&s.comm),
                zombie: s.status == 5,
                tty_dev: 0,
                start: 0.0,
            },
            false,
        ))
    }

    pub fn cwd(&mut self, pid: i32) -> String {
        // SAFETY: vnode buffer is VNODE_PATHINFO_SIZE bytes, the size the flavor needs.
        let n = unsafe {
            proc_pidinfo(
                pid,
                PROC_PIDVNODEPATHINFO,
                0,
                self.vnode.as_mut_ptr() as *mut c_void,
                VNODE_PATHINFO_SIZE as c_int,
            )
        };
        // A short read would leave the path bytes stale from the previous PID.
        if n == VNODE_PATHINFO_SIZE as c_int {
            cstr(&self.vnode[VNODE_INFO_LEN..VNODE_INFO_LEN + VNODE_PATH_LEN])
        } else {
            String::new()
        }
    }

    pub fn usage(&self, pid: i32) -> Option<Usage> {
        // rusage_info_v4 is 296 bytes; the buffer is larger so a bigger future layout cannot overflow.
        let mut buf = [0u64; 64];
        // SAFETY: buf is 512 bytes and 8-aligned.
        let r = unsafe { proc_pid_rusage(pid, RUSAGE_INFO_V4, buf.as_mut_ptr() as *mut c_void) };
        if r != 0 {
            return None;
        }
        // 16-byte uuid first, then u64 counters.
        let f = |i: usize| buf[2 + i];
        Some(Usage {
            footprint: f(7),
            resident: f(6),
            cpu_ticks: f(0).wrapping_add(f(1)),
            life_max_footprint: f(28),
        })
    }

    /// PID that macOS considers responsible for `pid` (private API; -1 if unavailable).
    pub fn responsible(&self, pid: i32) -> i32 {
        match self.responsible {
            Some(f) => f(pid),
            None => -1,
        }
    }

    /// argv and the allowlisted environment keys, via `KERN_PROCARGS2`.
    pub fn args(&mut self, pid: i32) -> Option<(Vec<String>, crate::model::Env)> {
        let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
        let mut len: libc::size_t = self.args.len();
        // SAFETY: args buffer is len bytes; mib has 3 entries.
        let r = unsafe {
            libc::sysctl(
                mib.as_mut_ptr(),
                3,
                self.args.as_mut_ptr() as *mut c_void,
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        };
        if r != 0 || len < 4 || len > self.args.len() {
            return None;
        }
        Some(parse_procargs(&self.args[..len]))
    }
}

/// Parses a KERN_PROCARGS2 buffer. Only allowlisted env keys are decoded.
///
/// Layout: `argc` (4 bytes), the exec path, NUL padding, then argv and env strings. The
/// kernel pads the exec path plus its NUL to a multiple of 8 bytes (checked on macOS 27
/// for every exe length seen). Only that padding is skipped, so an empty `argv[0]` is
/// kept; if the padding is shorter than expected, all NULs are skipped as before.
pub fn parse_procargs(raw: &[u8]) -> (Vec<String>, crate::model::Env) {
    let mut argv = Vec::new();
    let mut env = crate::model::Env::default();
    let Some((head, rest)) = raw.split_first_chunk::<4>() else {
        return (argv, env);
    };
    let argc = i32::from_ne_bytes(*head).max(0) as usize;
    let Some(exe_len) = rest.iter().position(|&c| c == 0) else {
        return (argv, env);
    };
    let nul_run = rest[exe_len..]
        .iter()
        .position(|&c| c != 0)
        .unwrap_or(rest.len() - exe_len);
    let padded = (exe_len + 1).next_multiple_of(8) - exe_len;
    let start = exe_len + if nul_run >= padded { padded } else { nul_run };
    let strings = rest.get(start..).unwrap_or(&[]);
    argv.reserve(argc.min(64));
    for (i, seg) in strings.split(|&c| c == 0).enumerate() {
        if i < argc {
            argv.push(String::from_utf8_lossy(seg).into_owned());
        } else if let Some(eq) = seg.iter().position(|&c| c == b'=') {
            let key = &seg[..eq];
            if ENV_ALLOW.iter().any(|k| k.as_bytes() == key) {
                let val: String = String::from_utf8_lossy(&seg[eq + 1..])
                    .chars()
                    .take(200)
                    .collect();
                match key {
                    b"T3CODE_THREAD_ID" => env.t3_thread = Some(val),
                    b"CLAUDE_CODE_ENTRYPOINT" => env.entrypoint = Some(val),
                    b"TMUX_PANE" => env.tmux_pane = Some(val),
                    b"PROCWHO_OWNER" => env.owner = Some(val),
                    _ => {}
                }
            }
        }
    }
    (argv, env)
}

fn argmax() -> usize {
    let mut mib = [libc::CTL_KERN, libc::KERN_ARGMAX];
    let mut val: c_int = 0;
    let mut len = std::mem::size_of::<c_int>();
    // SAFETY: val is a valid c_int out pointer.
    let r = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            2,
            &mut val as *mut c_int as *mut c_void,
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if r == 0 && val > 4096 {
        val as usize
    } else {
        1 << 20
    }
}

fn lookup_responsible() -> Option<extern "C" fn(c_int) -> c_int> {
    // Private libSystem call. Looked up at run time so a missing symbol cannot break linking.
    let name = b"responsibility_get_pid_responsible_for_pid\0";
    // SAFETY: name is NUL terminated; the symbol, if present, has the signature int(int).
    unsafe {
        let p = libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr() as *const c_char);
        if p.is_null() {
            None
        } else {
            Some(std::mem::transmute::<
                *mut c_void,
                extern "C" fn(c_int) -> c_int,
            >(p))
        }
    }
}

/// Mount points with a flag for local filesystems (`MNT_LOCAL` in `statfs.f_flags`).
/// Read with `getfsstat(MNT_NOWAIT)`, which returns cached data and contacts no
/// filesystem, so a hung network mount cannot block this call.
pub fn mounts() -> Vec<(String, bool)> {
    // SAFETY: a null buffer asks for the count only.
    let n = unsafe { libc::getfsstat(std::ptr::null_mut(), 0, libc::MNT_NOWAIT) };
    if n <= 0 {
        return Vec::new();
    }
    let cap = n as usize + 8;
    let mut v: Vec<libc::statfs> = Vec::with_capacity(cap);
    let bytes = (cap * std::mem::size_of::<libc::statfs>()) as c_int;
    // SAFETY: v has capacity for cap entries, which is what bytes describes.
    let got = unsafe { libc::getfsstat(v.as_mut_ptr(), bytes, libc::MNT_NOWAIT) };
    if got <= 0 {
        return Vec::new();
    }
    // SAFETY: the kernel initialized the first `got` entries (never more than cap).
    unsafe { v.set_len((got as usize).min(cap)) };
    v.iter()
        .map(|s| {
            let name: Vec<u8> = s.f_mntonname.iter().map(|&c| c as u8).collect();
            (cstr(&name), s.f_flags & libc::MNT_LOCAL as u32 != 0)
        })
        .collect()
}

/// Whether `path` is on a local filesystem, judged by the longest mount point that
/// contains it. Without a mount table the answer is true (the old behavior).
///
/// ```
/// use procwho_proc::sys::is_local;
/// let m = vec![("/".to_string(), true), ("/Volumes/share".to_string(), false)];
/// assert!(is_local(&m, "/Users/user"));
/// assert!(!is_local(&m, "/Volumes/share/x"));
/// assert!(is_local(&m, "/Volumes/shared"));
/// ```
pub fn is_local(mounts: &[(String, bool)], path: &str) -> bool {
    let contains = |m: &str| {
        m == "/" || path == m || (path.starts_with(m) && path[m.len()..].starts_with('/'))
    };
    mounts
        .iter()
        .filter(|(m, _)| contains(m))
        .max_by_key(|(m, _)| m.len())
        .is_none_or(|(_, local)| *local)
}

/// `ttys012` style name from a device number, or "".
pub fn tty_name(tdev: u32) -> String {
    if tdev == 0xFFFF_FFFF || tdev == 0 {
        String::new()
    } else if (tdev >> 24) == 16 {
        format!("ttys{:03}", tdev & 0xFF_FFFF)
    } else {
        format!("dev{:#x}", tdev)
    }
}

// ------------------------------------------------------------------ subprocess

/// Runs a read-only helper command. None on spawn failure, timeout or non-zero exit.
///
/// The helper runs in a new process group that this call creates. On timeout that group
/// is killed: it holds only procwho's own helper and whatever the helper started, never a
/// user process. This is the ONLY place procwho sends a signal. The call never waits
/// without a bound: if a grandchild escaped the group and still holds the pipe, the reader
/// thread is left behind instead of joined.
pub fn run(cmd: &[&str], timeout_s: u64) -> Option<String> {
    run_bounded(cmd, timeout_s, 8 * 1024 * 1024)
}

/// Like [`run`], with a hard stdout byte limit. Over-limit output is rejected, not truncated.
pub fn run_bounded(cmd: &[&str], timeout_s: u64, max_bytes: usize) -> Option<String> {
    let output = run_output(cmd, timeout_s, max_bytes).ok()?;
    (output.code == Some(0)).then_some(output.stdout)
}

/// Bounded helper output. Stderr is retained so collectors can distinguish an empty
/// match set from permission errors. Neither stream is logged or displayed verbatim.
#[derive(Debug)]
pub struct HelperOutput {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// Like [`run_bounded`], preserving exit status and bounded stderr for source policy.
pub fn run_output(
    cmd: &[&str],
    timeout_s: u64,
    max_bytes: usize,
) -> Result<HelperOutput, std::io::ErrorKind> {
    use std::io::ErrorKind;
    use std::os::unix::process::CommandExt;
    let (prog, args) = cmd.split_first().ok_or(ErrorKind::InvalidInput)?;
    let mut child = Command::new(prog)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .map_err(|e| e.kind())?;
    let out = child.stdout.take().ok_or(ErrorKind::Other)?;
    let err = child.stderr.take().ok_or(ErrorKind::Other)?;
    let (tx, rx) = mpsc::channel();
    for (is_error, mut stream) in [
        (false, Box::new(out) as Box<dyn Read + Send>),
        (true, Box::new(err) as Box<dyn Read + Send>),
    ] {
        let tx = tx.clone();
        // Not joined on timeout: an escaped grandchild may still hold a pipe.
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let ok = stream
                .by_ref()
                .take(max_bytes as u64 + 1)
                .read_to_end(&mut buf)
                .is_ok();
            let _ = tx.send((is_error, buf, ok));
        });
    }
    drop(tx);
    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_s);
    let kill_group = |child: &mut std::process::Child| {
        if let Ok(pgid) = libc::pid_t::try_from(child.id()) {
            // SAFETY: this is only our helper's new process group.
            unsafe { libc::killpg(pgid, libc::SIGKILL) };
        }
        let _ = child.wait();
    };
    let mut stdout = String::new();
    let mut stderr = String::new();
    for _ in 0..2 {
        let Ok((is_error, buf, ok)) =
            rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
        else {
            kill_group(&mut child);
            return Err(ErrorKind::TimedOut);
        };
        if !ok || buf.len() > max_bytes {
            kill_group(&mut child);
            return Err(ErrorKind::InvalidData);
        }
        let text = String::from_utf8_lossy(&buf).into_owned();
        if is_error {
            stderr = text;
        } else {
            stdout = text;
        }
    }
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(5))
            }
            _ => {
                kill_group(&mut child);
                return Err(ErrorKind::TimedOut);
            }
        }
    };
    Ok(HelperOutput {
        code: status.code(),
        stdout,
        stderr,
    })
}

// ------------------------------------------------------------------------ time

pub fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn localtime(t: f64) -> libc::tm {
    let secs = t.floor() as libc::time_t;
    // SAFETY: tm is plain data; localtime_r fills it.
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&secs, &mut tm);
        tm
    }
}

/// `strftime` in local time.
pub fn strftime(fmt: &str, t: f64) -> String {
    let tm = localtime(t);
    let cfmt = match std::ffi::CString::new(fmt) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };
    let mut buf = [0u8; 128];
    // SAFETY: buf is 128 bytes; strftime returns the number of bytes written.
    let n = unsafe {
        libc::strftime(
            buf.as_mut_ptr() as *mut c_char,
            buf.len(),
            cfmt.as_ptr(),
            &tm,
        )
    };
    String::from_utf8_lossy(&buf[..n]).into_owned()
}

/// Local-time broken-down fields to epoch seconds.
pub fn mktime_local(y: i32, mo: i32, d: i32, h: i32, mi: i32, s: i32) -> f64 {
    // SAFETY: tm is plain data; mktime normalizes it.
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        tm.tm_year = y - 1900;
        tm.tm_mon = mo - 1;
        tm.tm_mday = d;
        tm.tm_hour = h;
        tm.tm_min = mi;
        tm.tm_sec = s;
        tm.tm_isdst = -1;
        libc::mktime(&mut tm) as f64
    }
}

/// Own CPU time (user + system) in milliseconds.
pub fn own_cpu_ms() -> f64 {
    // SAFETY: rusage is plain data.
    unsafe {
        let mut ru: libc::rusage = std::mem::zeroed();
        libc::getrusage(libc::RUSAGE_SELF, &mut ru);
        let tv = |t: libc::timeval| t.tv_sec as f64 * 1e3 + t.tv_usec as f64 / 1e3;
        tv(ru.ru_utime) + tv(ru.ru_stime)
    }
}

/// Machine-wide numbers from sysctl, `getloadavg` and `statfs`. A value that cannot be read is 0.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SystemInfo {
    pub ncpu: u32,
    pub mem_total_bytes: u64,
    pub memory_pressure: Option<&'static str>,
    pub mem_used_bytes: Option<u64>,
    pub mem_available_bytes: Option<u64>,
    pub mem_compressed_bytes: Option<u64>,
    pub mem_wired_bytes: Option<u64>,
    pub swap_used_bytes: u64,
    pub swap_total_bytes: u64,
    pub swap_disk_free_bytes: u64,
    pub swap_disk_total_bytes: u64,
    pub load1: f64,
    pub disk_free_bytes: u64,
    pub disk_total_bytes: u64,
}

/// Activity Monitor-style physical use: anonymous non-purgeable + wired + compressor.
/// Cache pages are excluded; clamp only after signed subtraction.
pub fn memory_used_pages(counts: [u64; 4], page_size: u64, installed: u64) -> u64 {
    let [internal, wired, compressed, purgeable] = counts;
    let pages = (i128::from(internal) + i128::from(wired) + i128::from(compressed)
        - i128::from(purgeable))
    .max(0);
    (pages as u128 * u128::from(page_size)).min(u128::from(installed)) as u64
}

/// This sysctl returns dispatch flags (1/2/4), not XNU's internal enum (0/1/2/3).
pub fn pressure_level(raw: u32) -> Option<&'static str> {
    match raw {
        1 => Some("normal"),
        2 => Some("warning"),
        4 => Some("critical"),
        _ => None,
    }
}

fn physical_memory(si: &mut SystemInfo) {
    // SAFETY: the SDK's VM statistics layout and word count match HOST_VM_INFO64.
    // The host send right is released on all paths.
    unsafe {
        let host = mach_host_self();
        let mut vm: libc::vm_statistics64 = std::mem::zeroed();
        let mut count = libc::HOST_VM_INFO64_COUNT;
        let result = libc::host_statistics64(
            host,
            libc::HOST_VM_INFO64,
            (&mut vm as *mut libc::vm_statistics64).cast(),
            &mut count,
        );
        let mut page = 0;
        let page_result = host_page_size(host, &mut page);
        mach_port_deallocate(mach_task_self_, host);
        if result != libc::KERN_SUCCESS
            // Older macOS versions return a shorter supported revision. All counters
            // used here precede internal_page_count; later SDK additions are optional.
            || (count as usize * std::mem::size_of::<libc::integer_t>())
                < std::mem::offset_of!(libc::vm_statistics64, internal_page_count)
                    + std::mem::size_of::<u32>()
            || page_result != libc::KERN_SUCCESS
            || page == 0
        {
            return;
        }
        let page = page as u64;
        let used = memory_used_pages(
            [
                vm.internal_page_count.into(),
                vm.wire_count.into(),
                vm.compressor_page_count.into(),
                vm.purgeable_count.into(),
            ],
            page,
            si.mem_total_bytes,
        );
        si.mem_used_bytes = Some(used);
        si.mem_available_bytes = Some(si.mem_total_bytes.saturating_sub(used));
        si.mem_compressed_bytes = Some(u64::from(vm.compressor_page_count) * page);
        si.mem_wired_bytes = Some(u64::from(vm.wire_count) * page);
    }
}

/// Mirror of `struct xsw_usage` from `<sys/sysctl.h>` (`vm.swapusage`).
#[repr(C)]
#[derive(Default)]
struct XswUsage {
    total: u64,
    avail: u64,
    used: u64,
    pagesize: u32,
    encrypted: u8,
}

fn sysctl_by_name<T: Default>(name: &str) -> Option<T> {
    let cname = std::ffi::CString::new(name).ok()?;
    let mut v = T::default();
    let mut len = std::mem::size_of::<T>();
    // SAFETY: `v` is plain data of `len` bytes; the kernel writes at most `len` bytes.
    let rc = unsafe {
        libc::sysctlbyname(
            cname.as_ptr(),
            (&mut v as *mut T).cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    (rc == 0 && len == std::mem::size_of::<T>()).then_some(v)
}

/// Reads [`SystemInfo`]; `disk_path` is the directory whose filesystem is measured.
pub fn system_info(disk_path: &str) -> SystemInfo {
    let mut si = SystemInfo {
        ncpu: sysctl_by_name::<i32>("hw.ncpu").map_or(0, |n| n.max(0) as u32),
        mem_total_bytes: sysctl_by_name::<u64>("hw.memsize").unwrap_or(0),
        ..Default::default()
    };
    physical_memory(&mut si);
    si.memory_pressure =
        sysctl_by_name::<u32>("kern.memorystatus_vm_pressure_level").and_then(pressure_level);
    let swap_volume = std::ffi::CString::new("/private/var/vm").expect("static path");
    let mut swap_stat = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: path is NUL terminated; statfs initializes the output on success.
    if unsafe { libc::statfs(swap_volume.as_ptr(), swap_stat.as_mut_ptr()) } == 0 {
        let st = unsafe { swap_stat.assume_init() };
        si.swap_disk_free_bytes = st.f_bavail.saturating_mul(u64::from(st.f_bsize));
        si.swap_disk_total_bytes = st.f_blocks.saturating_mul(u64::from(st.f_bsize));
    }
    if let Some(x) = sysctl_by_name::<XswUsage>("vm.swapusage") {
        si.swap_total_bytes = x.total;
        si.swap_used_bytes = x.used;
    }
    let mut la = [0f64; 1];
    // SAFETY: the buffer holds the one sample that is requested.
    if unsafe { libc::getloadavg(la.as_mut_ptr(), 1) } == 1 {
        si.load1 = la[0];
    }
    if let Ok(c) = std::ffi::CString::new(disk_path) {
        // SAFETY: statfs is plain data; `c` is a valid C string.
        unsafe {
            let mut st: libc::statfs = std::mem::zeroed();
            if libc::statfs(c.as_ptr(), &mut st) == 0 {
                let bs = u64::from(st.f_bsize);
                si.disk_total_bytes = st.f_blocks * bs;
                si.disk_free_bytes = st.f_bavail * bs;
            }
        }
    }
    si
}

pub fn uid() -> u32 {
    // SAFETY: getuid has no preconditions.
    unsafe { libc::getuid() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pressure_mapping_rejects_internal_enum_and_unknown_flags() {
        assert_eq!(pressure_level(1), Some("normal"));
        assert_eq!(pressure_level(2), Some("warning"));
        assert_eq!(pressure_level(4), Some("critical"));
        for raw in [0, 3, 5, 8, u32::MAX] {
            assert_eq!(pressure_level(raw), None);
        }
    }

    #[test]
    fn physical_memory_formula_excludes_caches_and_clamps() {
        assert_eq!(
            memory_used_pages([100, 20, 10, 5], 4096, u64::MAX),
            125 * 4096
        );
        assert_eq!(memory_used_pages([0, 0, 0, 5], 4096, 100), 0);
        assert_eq!(memory_used_pages([100, 20, 10, 5], 4096, 100), 100);
    }

    #[test]
    fn procargs_reads_only_allowlisted_env() {
        let mut b = Vec::new();
        b.extend_from_slice(&2i32.to_ne_bytes());
        b.extend_from_slice(b"/bin/tool\0\0\0");
        b.extend_from_slice(b"tool\0--flag\0");
        b.extend_from_slice(
            b"T3CODE_THREAD_ID=abc123\0AWS_SECRET_ACCESS_KEY=hunter2\0PATH=/bin\0TMUX_PANE=%4\0",
        );
        let (argv, env) = parse_procargs(&b);
        assert_eq!(argv, vec!["tool", "--flag"]);
        assert_eq!(env.t3_thread.as_deref(), Some("abc123"));
        assert_eq!(env.tmux_pane.as_deref(), Some("%4"));
        assert!(env.owner.is_none() && env.entrypoint.is_none());
        assert!(!format!("{env:?}").contains("hunter2"));
    }

    #[test]
    fn run_returns_on_timeout_with_background_grandchild() {
        let t = std::time::Instant::now();
        assert_eq!(run(&["/bin/sh", "-c", "sleep 30 & sleep 30"], 1), None);
        assert!(
            t.elapsed() < Duration::from_millis(2500),
            "took {:?}",
            t.elapsed()
        );
    }

    #[test]
    fn run_reads_output() {
        assert_eq!(run(&["/bin/echo", "hi"], 5).as_deref(), Some("hi\n"));
        assert_eq!(run(&["/usr/bin/false"], 5), None);
        assert_eq!(run(&[], 5), None);
    }

    #[test]
    fn run_rejects_oversized_output() {
        assert_eq!(run_bounded(&["/bin/echo", "12345678"], 2, 4), None);
        assert_eq!(
            run_bounded(&["/bin/echo", "123"], 2, 4).as_deref(),
            Some("123\n")
        );
    }

    #[test]
    fn root_mount_is_local() {
        let m = mounts();
        assert!(m.iter().any(|(p, local)| p == "/" && *local), "{m:?}");
        assert!(is_local(&m, "/usr/bin"));
    }

    fn procargs(argc: i32, exe: &str, args: &[&str]) -> Vec<u8> {
        let mut b = argc.to_ne_bytes().to_vec();
        b.extend_from_slice(exe.as_bytes());
        let pad = (exe.len() + 1).next_multiple_of(8) - exe.len();
        b.extend(std::iter::repeat_n(0u8, pad));
        for a in args {
            b.extend_from_slice(a.as_bytes());
            b.push(0);
        }
        b
    }

    #[test]
    fn procargs_short_inputs_do_not_panic() {
        for raw in [
            &[][..],
            &[1][..],
            &[1, 0, 0][..],
            &[2, 0, 0, 0][..],
            &[2, 0, 0, 0, b'/', b'x'][..],
        ] {
            let (argv, _) = parse_procargs(raw);
            assert!(argv.is_empty(), "{raw:?}");
        }
        // argc larger than the data present
        assert_eq!(
            parse_procargs(&procargs(9, "/bin/x", &["x"])).0,
            vec!["x", ""]
        );
    }

    #[test]
    fn procargs_keeps_empty_leading_argv() {
        for exe in ["/bin/sleep", "/a", "/usr/bin/caffeinat", "/abcdef"] {
            let (argv, _) = parse_procargs(&procargs(2, exe, &["", "5"]));
            assert_eq!(argv, vec!["", "5"], "{exe}");
            let (argv, _) = parse_procargs(&procargs(3, exe, &["", "", "z"]));
            assert_eq!(argv, vec!["", "", "z"], "{exe}");
            let (argv, _) = parse_procargs(&procargs(2, exe, &["sleep", "5"]));
            assert_eq!(argv, vec!["sleep", "5"], "{exe}");
        }
    }

    #[test]
    fn cwd_of_self_and_of_missing_pid() {
        let mut rd = Reader::new();
        let me = std::process::id() as i32;
        let want = std::env::current_dir().unwrap().canonicalize().unwrap();
        assert_eq!(
            std::path::Path::new(&rd.cwd(me)).canonicalize().unwrap(),
            want
        );
        assert_eq!(rd.cwd(-5), "");
    }

    #[test]
    fn darwin_lp64_layouts_and_non_arm_timebases() {
        assert_eq!(std::mem::size_of::<BsdInfo>(), 136);
        assert_eq!(std::mem::size_of::<ShortBsdInfo>(), 64);
        assert_eq!(std::mem::size_of::<UniqInfo>(), 56);
        assert_eq!(ticks_to_ns(3, 125, 3), 125);
        assert_eq!(ticks_to_ns(123456789, 1, 1), 123456789);
        assert_eq!(ticks_to_ns(u64::MAX, u32::MAX, 1), u64::MAX);
        assert_eq!(ticks_to_ns(9, 1, 0), 9);
    }

    #[test]
    fn tty_names() {
        assert_eq!(tty_name(0), "");
        assert_eq!(tty_name(0xFFFF_FFFF), "");
        assert_eq!(tty_name((16 << 24) | 12), "ttys012");
    }
}
