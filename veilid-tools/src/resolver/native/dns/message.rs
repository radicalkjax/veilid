//! DNS wire format: response parsing for the record types veilid reads.
//!
//! Decoding is defensive: every read is bounds-checked, compression pointers must point
//! strictly backward, the hop cap and name length cap bound any pointer cycle, and any
//! malformed section rejects the whole message.

use super::*;

/// Fixed header: ID, FLAGS, QDCOUNT, ANCOUNT, NSCOUNT, ARCOUNT, all big-endian u16
pub(crate) const HEADER_LEN: usize = 12;

const FLAG_QR: u16 = 0x8000;
const FLAG_OPCODE_MASK: u16 = 0x7800;
const FLAG_OPCODE_SHIFT: u16 = 11;
const FLAG_TC: u16 = 0x0200;
const FLAG_RCODE_MASK: u16 = 0x000F;

const LABEL_KIND_MASK: u8 = 0xC0;
const LABEL_KIND_POINTER: u8 = 0xC0;
const POINTER_OFFSET_MASK: u8 = 0x3F;

pub(crate) const TYPE_PTR: u16 = 12;
pub(crate) const TYPE_TXT: u16 = 16;
pub(crate) const TYPE_OPT: u16 = 41;

pub(crate) const RCODE_NOERROR: u16 = 0;
pub(crate) const RCODE_NXDOMAIN: u16 = 3;

pub(crate) const MAX_NAME_LEN: usize = 255;
pub(crate) const MAX_POINTER_HOPS: usize = 16;

/// A question section entry
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Question {
    pub name: String,
    pub qtype: u16,
    pub qclass: u16,
}

/// Record data for the types veilid reads; everything else is skipped
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RData {
    Txt(Vec<Vec<u8>>),
    Ptr(String),
    Other,
}

/// A resource record from any section
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResourceRecord {
    pub name: String,
    pub rtype: u16,
    pub ttl: u32,
    pub rdata: RData,
}

/// A parsed DNS message
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Response {
    /// Transaction id from the header
    pub id: u16,
    /// QR flag: set on a response
    pub qr: bool,
    /// Header opcode
    pub opcode: u8,
    /// TC flag: the answer did not fit the transport
    pub truncated: bool,
    /// 12-bit response code including the EDNS extended bits
    pub rcode: u16,
    /// Question section
    pub questions: Vec<Question>,
    /// Answer section; authority and additional records are not kept
    pub answers: Vec<ResourceRecord>,
}

/// Parse a complete DNS message
pub(crate) fn parse_response(msg: &[u8]) -> Result<Response, DnsError> {
    if msg.len() < HEADER_LEN {
        return Err(DnsError::Malformed("short header"));
    }
    let id = read_u16(msg, 0)?;
    let flags = read_u16(msg, 2)?;
    let qdcount = read_u16(msg, 4)?;
    let ancount = read_u16(msg, 6)?;
    let nscount = read_u16(msg, 8)?;
    let arcount = read_u16(msg, 10)?;

    let mut pos = HEADER_LEN;
    let mut questions = Vec::new();
    for _ in 0..qdcount {
        let (name, next) = decode_name(msg, pos)?;
        let qtype = read_u16(msg, next)?;
        let qclass = read_u16(msg, next + 2)?;
        questions.push(Question {
            name,
            qtype,
            qclass,
        });
        pos = next + 4;
    }
    let mut answers = Vec::new();
    for _ in 0..ancount {
        let (rr, next) = parse_rr(msg, pos)?;
        answers.push(rr);
        pos = next;
    }
    for _ in 0..nscount {
        let (_, next) = parse_rr(msg, pos)?;
        pos = next;
    }
    let mut rcode = flags & FLAG_RCODE_MASK;
    for _ in 0..arcount {
        let (rr, next) = parse_rr(msg, pos)?;
        if rr.rtype == TYPE_OPT {
            // Extended rcode lives in the top byte of the OPT TTL
            rcode |= ((rr.ttl >> 24) as u16) << 4;
        }
        pos = next;
    }

    Ok(Response {
        id,
        qr: flags & FLAG_QR != 0,
        opcode: ((flags & FLAG_OPCODE_MASK) >> FLAG_OPCODE_SHIFT) as u8,
        truncated: flags & FLAG_TC != 0,
        rcode,
        questions,
        answers,
    })
}

fn read_u16(msg: &[u8], pos: usize) -> Result<u16, DnsError> {
    let b = msg
        .get(pos..pos + 2)
        .ok_or(DnsError::Malformed("truncated"))?;
    Ok(u16::from_be_bytes([b[0], b[1]]))
}

fn read_u32(msg: &[u8], pos: usize) -> Result<u32, DnsError> {
    let b = msg
        .get(pos..pos + 4)
        .ok_or(DnsError::Malformed("truncated"))?;
    Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

/// Decode a possibly compressed name at `start`, returning it with the position after it in the original stream
fn decode_name(msg: &[u8], start: usize) -> Result<(String, usize), DnsError> {
    let mut labels: Vec<u8> = Vec::new();
    let mut pos = start;
    let mut end = None;
    let mut hops = 0;
    // Root terminator counts toward the wire length limit
    let mut wire_len = 1;
    loop {
        let len = *msg.get(pos).ok_or(DnsError::Malformed("truncated name"))?;
        match len & LABEL_KIND_MASK {
            0 => {
                if len == 0 {
                    let name = String::from_utf8(labels)
                        .map_err(|_| DnsError::Malformed("non-ascii label"))?;
                    return Ok((name, end.unwrap_or(pos + 1)));
                }
                let len = len as usize;
                wire_len += 1 + len;
                if wire_len > MAX_NAME_LEN {
                    return Err(DnsError::Malformed("name too long"));
                }
                let label = msg
                    .get(pos + 1..pos + 1 + len)
                    .ok_or(DnsError::Malformed("truncated label"))?;
                // Names on the wire are ASCII; unicode arrives punycoded
                if !label.is_ascii() {
                    return Err(DnsError::Malformed("non-ascii label"));
                }
                if !labels.is_empty() {
                    labels.push(b'.');
                }
                labels.extend_from_slice(label);
                pos += 1 + len;
            }
            LABEL_KIND_POINTER => {
                let lo = *msg
                    .get(pos + 1)
                    .ok_or(DnsError::Malformed("truncated pointer"))?;
                let target = (usize::from(len & POINTER_OFFSET_MASK) << 8) | usize::from(lo);
                // A backward pointer can still cycle through labels; the hop cap ends that
                if target >= pos {
                    return Err(DnsError::Malformed("forward pointer"));
                }
                hops += 1;
                if hops > MAX_POINTER_HOPS {
                    return Err(DnsError::Malformed("pointer chain too long"));
                }
                if end.is_none() {
                    end = Some(pos + 2);
                }
                pos = target;
            }
            _ => return Err(DnsError::Malformed("label type")),
        }
    }
}

fn parse_rr(msg: &[u8], pos: usize) -> Result<(ResourceRecord, usize), DnsError> {
    let (name, pos) = decode_name(msg, pos)?;
    let rtype = read_u16(msg, pos)?;
    let ttl = read_u32(msg, pos + 4)?;
    let rdlen = usize::from(read_u16(msg, pos + 8)?);
    let rdata_pos = pos + 10;
    let rdata_end = rdata_pos + rdlen;
    let rdata_bytes = msg
        .get(rdata_pos..rdata_end)
        .ok_or(DnsError::Malformed("rdlength"))?;
    let rdata = match rtype {
        TYPE_TXT => RData::Txt(parse_txt(rdata_bytes)?),
        TYPE_PTR => {
            let (target, end) = decode_name(msg, rdata_pos)?;
            if end > rdata_end {
                return Err(DnsError::Malformed("ptr rdata"));
            }
            RData::Ptr(target)
        }
        _ => RData::Other,
    };
    Ok((
        ResourceRecord {
            name,
            rtype,
            ttl,
            rdata,
        },
        rdata_end,
    ))
}

/// TXT rdata is a sequence of length-prefixed character-strings that must exactly tile it
pub(crate) fn parse_txt(rdata: &[u8]) -> Result<Vec<Vec<u8>>, DnsError> {
    let mut strings = Vec::new();
    let mut pos = 0;
    while pos < rdata.len() {
        let len = usize::from(rdata[pos]);
        let s = rdata
            .get(pos + 1..pos + 1 + len)
            .ok_or(DnsError::Malformed("txt rdata"))?;
        strings.push(s.to_vec());
        pos += 1 + len;
    }
    Ok(strings)
}
