//! Answer decoding shared by the system-resolver backends that hand back wire-format messages.

use super::*;
use std::io;

pub(crate) mod message;

pub(crate) use message::*;

/// Failure of one lookup
#[derive(Debug)]
pub(crate) enum DnsError {
    /// The name cannot be handed to the resolver
    InvalidName(&'static str),
    /// An OS error on the resolver's descriptor
    Io(io::Error),
    /// No answer before the query timeout
    Timeout,
    /// The answer did not fit the transport
    Truncated,
    /// The server answered with a failure rcode
    Rcode(u16),
    /// NXDOMAIN, or NOERROR with no answers
    NoRecords { rcode: u16 },
    /// The answer does not decode
    Malformed(&'static str),
    /// The platform has no resolver for this record type
    Unsupported,
    /// A platform resolver error code
    Platform(i32),
}

impl fmt::Display for DnsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DnsError::InvalidName(what) => write!(f, "invalid name: {}", what),
            DnsError::Io(e) => write!(f, "io: {}", e),
            DnsError::Timeout => write!(f, "timeout"),
            DnsError::Truncated => write!(f, "truncated"),
            DnsError::Rcode(rcode) => write!(f, "{} (rcode {})", rcode_name(*rcode), rcode),
            DnsError::NoRecords { rcode } => write!(f, "no records ({})", rcode_name(*rcode)),
            DnsError::Malformed(what) => write!(f, "malformed: {}", what),
            DnsError::Unsupported => write!(f, "unsupported on this platform"),
            DnsError::Platform(code) => write!(f, "platform resolver error {}", code),
        }
    }
}

impl std::error::Error for DnsError {}

impl From<io::Error> for DnsError {
    fn from(e: io::Error) -> Self {
        DnsError::Io(e)
    }
}

fn rcode_name(rcode: u16) -> &'static str {
    match rcode {
        0 => "noerror",
        1 => "formerr",
        2 => "servfail",
        3 => "nxdomain",
        4 => "notimp",
        5 => "refused",
        16 => "badvers",
        _ => "rcode",
    }
}

/// Turn a reply into an answer or the failure it represents
pub(crate) fn classify(resp: Response) -> Result<Response, DnsError> {
    if resp.truncated {
        return Err(DnsError::Truncated);
    }
    match resp.rcode {
        RCODE_NOERROR if !resp.answers.is_empty() => Ok(resp),
        RCODE_NOERROR | RCODE_NXDOMAIN => Err(DnsError::NoRecords { rcode: resp.rcode }),
        rcode => Err(DnsError::Rcode(rcode)),
    }
}

/// Each TXT record's character-strings joined into one string; records that are not UTF-8 are dropped
#[must_use]
pub(crate) fn txt_records(resp: &Response) -> Vec<String> {
    resp.answers
        .iter()
        .filter_map(|rr| match &rr.rdata {
            RData::Txt(strings) => String::from_utf8(strings.concat()).ok(),
            _ => None,
        })
        .collect()
}
