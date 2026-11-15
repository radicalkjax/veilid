/// A growable byte buffer an AEAD seals or opens in place: sealing appends the tag and
/// opening truncates it off.
pub trait AeadBuffer: AsRef<[u8]> + AsMut<[u8]> {
    /// Bytes held
    fn len(&self) -> usize;

    /// Whether nothing is held
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Append `other`
    fn extend_from_slice(&mut self, other: &[u8]);

    /// Drop everything past `len`
    fn truncate(&mut self, len: usize);
}

impl AeadBuffer for Vec<u8> {
    fn len(&self) -> usize {
        Vec::<u8>::len(self)
    }

    fn is_empty(&self) -> bool {
        Vec::<u8>::is_empty(self)
    }

    fn extend_from_slice(&mut self, other: &[u8]) {
        Vec::<u8>::extend_from_slice(self, other);
    }

    fn truncate(&mut self, len: usize) {
        Vec::<u8>::truncate(self, len);
    }
}
