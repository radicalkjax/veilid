use super::*;
#[cfg(feature = "backtrace")]
use std::backtrace::Backtrace;
use veilid_core_optimized::{lz4_compress_prepend_size, lz4_decompress, lz4_uncompressed_size};

impl_veilid_log_facility!("compression");

#[cfg_attr(
    feature = "instrument",
    instrument(level = "trace", target = "compression", skip_all)
)]
#[must_use]
pub(crate) fn compress_prepend_size(input: &[u8]) -> Vec<u8> {
    lz4_compress_prepend_size(input)
}

#[cfg_attr(
    feature = "instrument",
    instrument(level = "trace", target = "compression", skip_all, err)
)]
pub(crate) fn decompress_size_prepended(input: &[u8], max_size: usize) -> VeilidAPIResult<Vec<u8>> {
    let (uncompressed_size, input) = match lz4_uncompressed_size(input) {
        Ok(v) => v,
        Err(e) => {
            apibail_generic!("failed to get uncompressed size: {}", e);
        }
    };
    if uncompressed_size > max_size {
        apibail_generic!(
            "decompression exceeded maximum size: {} > {}",
            uncompressed_size,
            max_size
        );
    }
    match lz4_decompress(input, uncompressed_size) {
        Ok(v) => Ok(v),
        Err(e) => {
            #[cfg(feature = "backtrace")]
            apibail_generic!(
                "failed to decompress: {}:data_len={}\ndata={}\nbacktrace:\n{}",
                e,
                input.len(),
                human_byte_data(input, Some(256)),
                Backtrace::force_capture()
            );
            #[cfg(not(feature = "backtrace"))]
            apibail_generic!(
                "failed to decompress: {}:data_len={}\ndata={}",
                e,
                input.len(),
                human_byte_data(input, Some(256))
            );
        }
    }
}
