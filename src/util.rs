#[cfg(not(feature = "tcp"))]
use crate::connection::CoreRead;
#[cfg(not(feature = "tcp"))]
use crate::Error;
#[cfg(not(feature = "tcp"))]
use alloc::vec::Vec;

use core::fmt::{self, Write};

pub(crate) struct VecWriter<'a> {
    buffer: &'a mut Vec<u8>,
}

impl<'a> VecWriter<'a> {
    pub fn new(buffer: &'a mut Vec<u8>) -> Self {
        VecWriter { buffer }
    }
}

impl<'a> Write for VecWriter<'a> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.buffer.extend_from_slice(s.as_bytes());
        Ok(())
    }
}


#[cfg(not(feature = "tcp"))]
pub(crate) struct BufferReader<T: CoreRead> {
    backing_stream: T,
    backing_buffer: Vec<u8>,
    index: usize,
}

#[cfg(not(feature = "tcp"))]
impl<T: CoreRead> BufferReader<T> {
    pub(crate) fn new(max_size: usize, stream: T) -> BufferReader<T> {
        let backer = Vec::with_capacity(max_size);
        BufferReader {
            backing_stream: stream,
            backing_buffer: backer,
            index: 0,
        }
    }
}

#[cfg(not(feature = "tcp"))]
impl<T: CoreRead> Iterator for BufferReader<T> {
    type Item = Result<u8, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        // TODO if index < baacking_buffer size, clear and read more.
        if self.index >= self.backing_buffer.len() {
            let stream = &mut self.backing_stream;
            let buf = &mut self.backing_buffer;
            let _ = CoreRead::read(stream, buf);
            self.index = 0;
        }
        if self.index < self.backing_buffer.len() {
            // If we're in bounds now, return a byte
            self.index += 1;
            Some(Ok(self.backing_buffer[self.index - 1]))
        } else {
            // If can't read any more, return None
            None
        }
    }
}
