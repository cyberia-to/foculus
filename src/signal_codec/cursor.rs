use super::{CodecError, ErrorKind, MAX_SIGNAL_BYTES};

pub(crate) struct Reader<'a> {
    pub(crate) bytes: &'a [u8],
    pub(crate) position: usize,
    base: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8], base: usize) -> Self {
        Self {
            bytes,
            position: 0,
            base,
        }
    }
    pub(crate) fn error(&self, kind: ErrorKind) -> CodecError {
        CodecError {
            position: self.base + self.position,
            kind,
        }
    }
    pub(crate) fn remaining(&self) -> usize {
        self.bytes.len() - self.position
    }
    pub(crate) fn take(&mut self, count: usize) -> Result<&'a [u8], CodecError> {
        if count > self.remaining() {
            return Err(self.error(ErrorKind::Truncated));
        }
        let start = self.position;
        self.position += count;
        Ok(&self.bytes[start..self.position])
    }
    pub(crate) fn array<const N: usize>(&mut self) -> Result<[u8; N], CodecError> {
        self.take(N)?
            .try_into()
            .map_err(|_| self.error(ErrorKind::Truncated))
    }
    pub(crate) fn byte(&mut self) -> Result<u8, CodecError> {
        Ok(self.array::<1>()?[0])
    }
    pub(crate) fn u32(&mut self) -> Result<u32, CodecError> {
        Ok(u32::from_le_bytes(self.array()?))
    }
    pub(crate) fn u64(&mut self) -> Result<u64, CodecError> {
        Ok(u64::from_le_bytes(self.array()?))
    }
    pub(crate) fn flag(&mut self) -> Result<bool, CodecError> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(self.error(ErrorKind::Invalid("option tag"))),
        }
    }
    pub(crate) fn count(&mut self, max: usize, min_bytes: usize) -> Result<usize, CodecError> {
        let count = self.u32()? as usize;
        if count > max {
            return Err(self.error(ErrorKind::Limit("collection count")));
        }
        if count > self.remaining() / min_bytes {
            return Err(self.error(ErrorKind::Truncated));
        }
        Ok(count)
    }
    pub(crate) fn finish(&self) -> Result<(), CodecError> {
        if self.remaining() != 0 {
            return Err(self.error(ErrorKind::Trailing));
        }
        Ok(())
    }
    pub(crate) fn vector<T>(&self, count: usize) -> Result<Vec<T>, CodecError> {
        let mut vector = Vec::new();
        vector
            .try_reserve_exact(count)
            .map_err(|_| self.error(ErrorKind::Limit("allocation")))?;
        Ok(vector)
    }
}

pub(crate) struct Writer {
    pub(crate) bytes: Vec<u8>,
}
impl Writer {
    pub(crate) fn new() -> Self {
        Self { bytes: Vec::new() }
    }
    pub(crate) fn error(&self, kind: ErrorKind) -> CodecError {
        CodecError {
            position: self.bytes.len(),
            kind,
        }
    }
    pub(crate) fn put(&mut self, bytes: &[u8]) -> Result<(), CodecError> {
        if bytes.len() > MAX_SIGNAL_BYTES - self.bytes.len() {
            return Err(self.error(ErrorKind::Limit("signal bytes")));
        }
        self.bytes
            .try_reserve(bytes.len())
            .map_err(|_| self.error(ErrorKind::Limit("allocation")))?;
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
    pub(crate) fn byte(&mut self, value: u8) -> Result<(), CodecError> {
        self.put(&[value])
    }
    pub(crate) fn u64(&mut self, value: u64) -> Result<(), CodecError> {
        self.put(&value.to_le_bytes())
    }
    pub(crate) fn count(&mut self, value: usize, max: usize) -> Result<(), CodecError> {
        if value > max {
            return Err(self.error(ErrorKind::Limit("collection count")));
        }
        let value = u32::try_from(value).map_err(|_| self.error(ErrorKind::Limit("u32 count")))?;
        self.put(&value.to_le_bytes())
    }
}
