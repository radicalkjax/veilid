use super::*;

// Wire and info constants, isolated because the HPKE RFC is still in review
pub(crate) const HPKE_VERSION_1: u8 = 1;
pub(crate) const HPKE_INFO_LABEL: &[u8] = b"veilid-hpke/1";
pub(crate) const HPKE_HEADER_LENGTH: usize = 5;

pub(crate) fn hpke_info(kind: CryptoKind) -> Vec<u8> {
    let mut info = Vec::with_capacity(HPKE_INFO_LABEL.len() + 4);
    info.extend_from_slice(HPKE_INFO_LABEL);
    info.extend_from_slice(kind.bytes());
    info
}

// `enc` is RFC 9180's per-seal "encapsulated key" (a KEM ciphertext), NOT FIPS 203's
// "encapsulation key" (the KEM public key, our EncapsulationKey); keep it raw bytes
pub(crate) fn build_sealed(kind: CryptoKind, enc: &[u8], ciphertext: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HPKE_HEADER_LENGTH + enc.len() + ciphertext.len());
    out.push(HPKE_VERSION_1);
    out.extend_from_slice(kind.bytes());
    out.extend_from_slice(enc);
    out.extend_from_slice(ciphertext);
    out
}

pub(crate) fn parse_sealed(
    kind: CryptoKind,
    enc_length: usize,
    sealed: &[u8],
) -> VeilidAPIResult<(&[u8], &[u8])> {
    if sealed.len() < HPKE_HEADER_LENGTH {
        apibail_parse_error!("sealed blob is truncated", sealed.len());
    }
    if sealed[0] != HPKE_VERSION_1 {
        apibail_parse_error!("unsupported hpke version", sealed[0]);
    }
    let blob_kind = CryptoKind::try_from(&sealed[1..HPKE_HEADER_LENGTH])?;
    if blob_kind != kind {
        apibail_invalid_argument!(
            "sealed blob crypto kind does not match",
            "sealed",
            blob_kind
        );
    }
    if sealed.len() < HPKE_HEADER_LENGTH + enc_length {
        apibail_parse_error!("sealed blob is truncated", sealed.len());
    }
    Ok(sealed[HPKE_HEADER_LENGTH..].split_at(enc_length))
}
