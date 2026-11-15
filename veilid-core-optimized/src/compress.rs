use crate::Error;
use lz4_flex::block;

/// LZ4 block of `input`, prefixed with its uncompressed size
#[must_use]
pub fn lz4_compress_prepend_size(input: &[u8]) -> Vec<u8> {
    block::compress_prepend_size(input)
}

/// The uncompressed size a size-prefixed LZ4 block declares, and the block after the prefix
pub fn lz4_uncompressed_size(input: &[u8]) -> Result<(usize, &[u8]), Error> {
    block::uncompressed_size(input).map_err(|e| Error::Decompress(e.to_string()))
}

/// Decode an LZ4 block of known `uncompressed_size`
pub fn lz4_decompress(input: &[u8], uncompressed_size: usize) -> Result<Vec<u8>, Error> {
    block::decompress(input, uncompressed_size).map_err(|e| Error::Decompress(e.to_string()))
}
