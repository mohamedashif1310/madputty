//! Abstraction over serial port I/O for testability.
//!
//! `DuplexStream` is a trait that `session.rs` uses instead of directly
//! depending on `Box<dyn SerialPort>`. This lets integration tests inject
//! a mock backed by in-memory pipes.

use std::io::{self, Read, Write};

/// A bidirectional byte stream — read from device, write to device.
/// Real impl: serialport::SerialPort. Test impl: in-memory pipe pair.
pub trait DuplexStream: Read + Write + Send + 'static {
    /// Clone the stream into a separate read handle (for the port_reader task).
    /// Returns None if cloning is not supported.
    fn try_clone_stream(&self) -> io::Result<Box<dyn DuplexStream>>;
}

/// Wrapper around `Box<dyn serialport::SerialPort>` implementing DuplexStream.
pub struct SerialPortStream {
    inner: Box<dyn serialport::SerialPort>,
}

impl SerialPortStream {
    pub fn new(port: Box<dyn serialport::SerialPort>) -> Self {
        Self { inner: port }
    }
}

impl Read for SerialPortStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

impl Write for SerialPortStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl DuplexStream for SerialPortStream {
    fn try_clone_stream(&self) -> io::Result<Box<dyn DuplexStream>> {
        let cloned = self.inner.try_clone()?;
        Ok(Box::new(SerialPortStream { inner: cloned }))
    }
}
