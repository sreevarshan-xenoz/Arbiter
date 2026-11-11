//! Windows-only pieces: the virtio-serial port and the Win32 probe.
//!
//! The port is opened for overlapped I/O. On a synchronous handle Windows serialises all I/O on
//! the file object, so a read waiting for the host would also block every write; overlapped reads
//! with a timeout avoid that and let the agent notice a missing host.
//!
//! The agent must run in the logged-in user's session (Task Scheduler "at logon"), not as a
//! service: services live in session 0, where the foreground window and last-input time of the
//! user's desktop are not visible.

use crate::port::Port;
use crate::report::{Probe, Sample};
use std::io;
use std::ptr::{null, null_mut};
use std::time::Duration;
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_IO_PENDING, FILETIME, GENERIC_READ, GENERIC_WRITE, HANDLE,
    INVALID_HANDLE_VALUE, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, WriteFile, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_OVERLAPPED, OPEN_EXISTING,
};
use windows_sys::Win32::System::SystemInformation::{
    GetTickCount, GlobalMemoryStatusEx, MEMORYSTATUSEX,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, GetSystemTimes, OpenProcess, QueryFullProcessImageNameW, WaitForSingleObject,
    PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

/// Name the virtio-serial driver (vioser) gives the port from the domain XML's
/// `<target type='virtio' name='org.arbiter.agent.0'/>`.
pub const DEFAULT_PORT: &str = r"\\.\Global\org.arbiter.agent.0";
const WRITE_TIMEOUT: Duration = Duration::from_secs(2);

pub struct WinPort {
    handle: HANDLE,
    event: HANDLE,
}

// SAFETY: the handles are owned by this struct and only used through &mut self.
unsafe impl Send for WinPort {}

impl WinPort {
    pub fn open(path: &str) -> io::Result<Self> {
        let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: `wide` is NUL-terminated and outlives the call; null security attributes/template are allowed.
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OVERLAPPED,
                null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        // Manual-reset event, initially unsignalled; ReadFile/WriteFile reset it when they start.
        // SAFETY: plain Win32 call with valid (null) optional arguments.
        let event = unsafe { CreateEventW(null(), 1, 0, null()) };
        if event.is_null() {
            let e = io::Error::last_os_error();
            // SAFETY: `handle` is valid and owned here.
            unsafe { CloseHandle(handle) };
            return Err(e);
        }
        Ok(Self { handle, event })
    }

    /// One overlapped read or write, waiting at most `timeout`. A timed-out read returns whatever
    /// arrived (possibly 0 bytes); a timed-out write is an error.
    fn io(
        &mut self,
        write: bool,
        ptr: *mut u8,
        len: usize,
        timeout: Duration,
    ) -> io::Result<usize> {
        let len = u32::try_from(len).unwrap_or(u32::MAX);
        // SAFETY: OVERLAPPED is plain data; all-zero is its documented initial state.
        let mut ov: OVERLAPPED = unsafe { std::mem::zeroed() };
        ov.hEvent = self.event;
        // SAFETY: `ptr`/`len` describe a live buffer owned by the caller that stays borrowed until
        // this function returns, and we never return before the operation has completed or been
        // cancelled-and-waited-for, so the kernel never touches `ov` or the buffer afterwards.
        let ok = unsafe {
            if write {
                WriteFile(self.handle, ptr, len, null_mut(), &mut ov)
            } else {
                ReadFile(self.handle, ptr, len, null_mut(), &mut ov)
            }
        };
        if ok == 0 {
            // SAFETY: no preconditions.
            let err = unsafe { GetLastError() };
            if err != ERROR_IO_PENDING {
                return Err(io::Error::from_raw_os_error(err as i32));
            }
        }
        let ms = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
        // SAFETY: valid event handle.
        let waited = unsafe { WaitForSingleObject(self.event, ms) };
        let mut n: u32 = 0;
        if waited == WAIT_TIMEOUT {
            // SAFETY: cancel our own pending operation, then block until it is really finished
            // so `ov` and the buffer can be released.
            unsafe {
                CancelIoEx(self.handle, &ov);
                GetOverlappedResult(self.handle, &ov, &mut n, 1);
            }
            return if write {
                Err(io::ErrorKind::TimedOut.into())
            } else {
                Ok(n as usize)
            };
        }
        if waited != WAIT_OBJECT_0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the operation has completed (event signalled).
        if unsafe { GetOverlappedResult(self.handle, &ov, &mut n, 0) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(n as usize)
    }
}

impl Port for WinPort {
    fn send(&mut self, bytes: &[u8]) -> io::Result<()> {
        let mut off = 0;
        while off < bytes.len() {
            let rest = &bytes[off..];
            // WriteFile only reads from the buffer; the *mut cast is for the shared helper.
            let n = self.io(true, rest.as_ptr() as *mut u8, rest.len(), WRITE_TIMEOUT)?;
            if n == 0 {
                return Err(io::ErrorKind::WriteZero.into());
            }
            off += n;
        }
        Ok(())
    }

    fn recv(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<usize> {
        let n = self.io(false, buf.as_mut_ptr(), buf.len(), timeout)?;
        if n == 0 {
            // A zero-byte completion can come back immediately (e.g. host not connected);
            // don't spin on it.
            std::thread::sleep(timeout.min(Duration::from_millis(50)));
        }
        Ok(n)
    }
}

impl Drop for WinPort {
    fn drop(&mut self) {
        // SAFETY: both handles are valid and owned; no operation is pending (io() always waits).
        unsafe {
            CloseHandle(self.handle);
            CloseHandle(self.event);
        }
    }
}

/// Foreground app, CPU, memory and input idle time from Win32.
pub struct WinProbe;

fn ft(f: FILETIME) -> u64 {
    (u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime)
}

impl WinProbe {
    fn cpu_times() -> Option<(u64, u64)> {
        let zero = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let (mut idle, mut kernel, mut user) = (zero, zero, zero);
        // SAFETY: valid out-pointers.
        if unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } == 0 {
            return None;
        }
        // Kernel time includes idle time.
        let total = ft(kernel) + ft(user);
        Some((total.saturating_sub(ft(idle)), total))
    }

    fn mem_used_mb() -> u64 {
        // SAFETY: plain data; dwLength set before the call as the API requires.
        let mut m: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
        m.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        // SAFETY: valid out-pointer.
        if unsafe { GlobalMemoryStatusEx(&mut m) } == 0 {
            return 0;
        }
        m.ullTotalPhys.saturating_sub(m.ullAvailPhys) / (1024 * 1024)
    }

    fn idle_secs() -> u64 {
        let mut li = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        // SAFETY: valid out-pointer with cbSize set.
        if unsafe { GetLastInputInfo(&mut li) } == 0 {
            return 0;
        }
        // Both are 32-bit tick counts; wrapping_sub stays right across the 49-day wrap.
        // SAFETY: no preconditions.
        u64::from(unsafe { GetTickCount() }.wrapping_sub(li.dwTime) / 1000)
    }

    fn foreground() -> Option<String> {
        // SAFETY: no preconditions; may return null.
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.is_null() {
            return None;
        }
        let mut pid = 0u32;
        // SAFETY: valid window handle and out-pointer.
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if pid == 0 {
            return None;
        }
        // SAFETY: plain Win32 call; null on failure (e.g. elevated process).
        let h = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if h.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        // SAFETY: `buf` has `len` u16 slots; `h` is valid and closed right after.
        let ok = unsafe {
            QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len)
        };
        // SAFETY: `h` is valid and owned here.
        unsafe { CloseHandle(h) };
        if ok == 0 {
            return None;
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        path.rsplit('\\').next().map(str::to_string)
    }
}

impl Probe for WinProbe {
    fn sample(&mut self) -> Sample {
        Sample {
            foreground: Self::foreground(),
            cpu_times: Self::cpu_times(),
            mem_used_mb: Self::mem_used_mb(),
            idle_secs: Self::idle_secs(),
        }
    }
}
