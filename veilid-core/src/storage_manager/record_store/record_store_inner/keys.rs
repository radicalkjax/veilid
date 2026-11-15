use super::*;

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct RecordTableKey {
    pub record_key: OpaqueRecordKey,
}
impl RecordTableKey {
    pub fn bytes(&self) -> Vec<u8> {
        Vec::from(self.record_key.clone())
    }
}
impl fmt::Display for RecordTableKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.record_key)
    }
}

impl TryFrom<&[u8]> for RecordTableKey {
    type Error = EyreReport;
    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let key = OpaqueRecordKey::try_from(bytes)?;
        Ok(RecordTableKey { record_key: key })
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct SubkeyTableKey {
    pub record_key: OpaqueRecordKey,
    pub subkey: ValueSubkey,
}
impl fmt::Display for SubkeyTableKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.record_key, self.subkey)
    }
}
impl SubkeyTableKey {
    /// In-memory footprint, including the record key bytes
    pub fn total_size(&self) -> usize {
        mem::size_of::<Self>() + self.record_key.ref_value().len()
    }
    pub fn bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::<_>::from(self.record_key.clone());
        bytes.extend_from_slice(&self.subkey.to_le_bytes());
        bytes
    }
}
impl TryFrom<&[u8]> for SubkeyTableKey {
    type Error = EyreReport;
    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let Some(key_len) = bytes.len().checked_sub(4) else {
            bail!("subkey table key too short");
        };
        let key = OpaqueRecordKey::try_from(&bytes[..key_len])?;
        let subkey =
            ValueSubkey::from_le_bytes(bytes[key_len..].try_into().wrap_err("invalid subkey")?);

        Ok(SubkeyTableKey {
            record_key: key,
            subkey,
        })
    }
}
