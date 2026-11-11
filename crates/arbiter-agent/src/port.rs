//! Byte transport to arbiterd. On Windows it is the virtio-serial port (`win::WinPort`); on Linux a
//! unix socket stands in for it (`--dev-listen` and tests).

use std::io;
use std::time::Duration;

pub trait Port {
    fn send(&mut self, bytes: &[u8]) -> io::Result<()>;
    /// Wait up to `timeout` for bytes. `Ok(0)` means nothing arrived; a closed peer is an error.
    fn recv(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<usize>;
}

#[cfg(unix)]
pub use unix::UnixPort;

#[cfg(unix)]
mod unix {
    use super::*;
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;

    pub struct UnixPort(UnixStream);

    impl UnixPort {
        pub fn new(s: UnixStream) -> Self {
            Self(s)
        }
    }

    impl Port for UnixPort {
        fn send(&mut self, bytes: &[u8]) -> io::Result<()> {
            self.0.set_write_timeout(Some(Duration::from_secs(2)))?;
            self.0.write_all(bytes)
        }

        fn recv(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<usize> {
            self.0
                .set_read_timeout(Some(timeout.max(Duration::from_millis(1))))?;
            match self.0.read(buf) {
                Ok(0) => Err(io::ErrorKind::UnexpectedEof.into()),
                Ok(n) => Ok(n),
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    Ok(0)
                }
                Err(e) => Err(e),
            }
        }
    }
}
