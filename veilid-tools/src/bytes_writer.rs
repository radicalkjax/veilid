use std::{cmp::min, io::Write};

use super::*;

/// A [`std::io::Write`] adapter over a [`BytesMut`] buffer.
///
/// Writes overwrite existing bytes from the current position and extend the buffer past its end,
/// growing it as needed. Tracks the starting offset so [`BytesWriter::bytes_written`] reports only
/// bytes produced by this writer.
#[must_use]
pub struct BytesWriter {
    bytes_mut: BytesMut,
    start: usize,
    position: usize,
}

impl Default for BytesWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl BytesWriter {
    /// Create an empty writer.
    pub fn new() -> Self {
        Self::with_capacity(0)
    }

    /// Create an empty writer with the given preallocated buffer capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            bytes_mut: BytesMut::with_capacity(capacity),
            start: 0,
            position: 0,
        }
    }

    /// Wrap an existing buffer, appending after its current contents.
    pub fn new_append(bytes_mut: BytesMut) -> Self {
        let start = bytes_mut.len();
        Self {
            bytes_mut,
            start,
            position: start,
        }
    }

    /// Wrap an existing buffer, writing from `offset`. The buffer is zero-padded up to `offset` if shorter.
    pub fn new_offset(mut bytes_mut: BytesMut, offset: usize) -> Self {
        if offset > bytes_mut.len() {
            bytes_mut.resize(offset, 0);
        }
        Self {
            bytes_mut,
            start: offset,
            position: offset,
        }
    }

    /// Number of bytes written since the starting offset.
    #[must_use]
    pub fn bytes_written(&self) -> usize {
        self.position - self.start
    }
    /// Current write position in the buffer.
    #[must_use]
    pub fn position(&self) -> usize {
        self.position
    }
    /// Offset at which this writer began writing.
    #[must_use]
    pub fn start(&self) -> usize {
        self.start
    }
    /// Borrow the underlying buffer.
    #[must_use]
    pub fn bytes_mut(&self) -> &BytesMut {
        &self.bytes_mut
    }

    /// Consume the writer and return the underlying buffer.
    #[must_use]
    pub fn into_inner(self) -> BytesMut {
        self.bytes_mut
    }
}

impl Write for BytesWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let len = buf.len();

        let len_to_copy = min(len, self.bytes_mut.len() - self.position);
        if len_to_copy > 0 {
            self.bytes_mut[self.position..self.position + len_to_copy]
                .copy_from_slice(&buf[..len_to_copy]);
        }
        if len_to_copy < len {
            self.bytes_mut.extend_from_slice(&buf[len_to_copy..]);
        }
        self.position += len;
        Ok(len)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
