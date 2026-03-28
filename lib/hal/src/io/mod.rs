pub use error::IoError;

mod error;

pub enum SeekFrom {
    Start(u64),
    Current(i64),
    End(i64),
}

pub trait Read {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, IoError>;

    fn read_exact(&mut self, mut buf: &mut [u8]) -> Result<(), IoError> {
        while !buf.is_empty() {
            match self.read(buf) {
                Ok(0) => return Err(IoError::UnexpectedEof),
                Ok(n) => buf = &mut buf[n..],
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

pub trait Write {
    fn write(&mut self, buf: &[u8]) -> Result<usize, IoError>;
    fn flush(&mut self) -> Result<(), IoError>;
}

pub trait Seek {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64, IoError>;

    fn stream_position(&mut self) -> Result<u64, IoError> { self.seek(SeekFrom::Current(0)) }
}
