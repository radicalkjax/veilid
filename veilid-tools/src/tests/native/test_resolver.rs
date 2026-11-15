use crate::resolver::native::dns::message::*;
use crate::resolver::native::dns::{classify, txt_records, DnsError};
use crate::*;

const CLASS_IN: u16 = 1;

//////////////////////////////////////////////////////////////////////////////////
// Wire format fixtures

/// Append `name` in wire form (length-prefixed labels, root terminator)
fn encode_name(name: &str, out: &mut Vec<u8>) {
    for label in name
        .trim_end_matches('.')
        .split('.')
        .filter(|l| !l.is_empty())
    {
        out.push(label.len() as u8);
        out.extend_from_slice(label.as_bytes());
    }
    out.push(0);
}

#[derive(Clone)]
struct Answer {
    name: String,
    rtype: u16,
    rdata: Vec<u8>,
}

impl Answer {
    fn txt(name: &str, strings: &[&[u8]]) -> Self {
        let mut rdata = Vec::new();
        for s in strings {
            rdata.push(s.len() as u8);
            rdata.extend_from_slice(s);
        }
        Self {
            name: name.to_string(),
            rtype: TYPE_TXT,
            rdata,
        }
    }

    fn ptr(name: &str, target: &str) -> Self {
        let mut rdata = Vec::new();
        encode_name(target, &mut rdata);
        Self {
            name: name.to_string(),
            rtype: TYPE_PTR,
            rdata,
        }
    }
}

struct ResponseBuilder {
    id: u16,
    flags: u16,
    question: (String, u16),
    answers: Vec<Answer>,
    additional: Vec<Answer>,
}

impl ResponseBuilder {
    fn new(id: u16, name: &str, qtype: u16) -> Self {
        Self {
            id,
            // QR + RD + RA
            flags: 0x8180,
            question: (name.to_string(), qtype),
            answers: Vec::new(),
            additional: Vec::new(),
        }
    }

    fn flags(mut self, flags: u16) -> Self {
        self.flags = flags;
        self
    }

    fn rcode(mut self, rcode: u16) -> Self {
        self.flags = (self.flags & !0x000F) | rcode;
        self
    }

    fn answer(mut self, answer: Answer) -> Self {
        self.answers.push(answer);
        self
    }

    fn opt(mut self, ttl: u32) -> Self {
        self.additional.push(Answer {
            name: String::new(),
            rtype: TYPE_OPT,
            rdata: ttl.to_be_bytes().to_vec(),
        });
        self
    }

    fn build(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&self.id.to_be_bytes());
        out.extend_from_slice(&self.flags.to_be_bytes());
        out.extend_from_slice(&1u16.to_be_bytes());
        out.extend_from_slice(&(self.answers.len() as u16).to_be_bytes());
        out.extend_from_slice(&0u16.to_be_bytes());
        out.extend_from_slice(&(self.additional.len() as u16).to_be_bytes());
        encode_name(&self.question.0, &mut out);
        out.extend_from_slice(&self.question.1.to_be_bytes());
        out.extend_from_slice(&CLASS_IN.to_be_bytes());
        for rr in self.answers.iter().chain(self.additional.iter()) {
            if rr.rtype == TYPE_OPT {
                // OPT carries its extended rcode and version in the TTL field
                out.push(0);
                out.extend_from_slice(&TYPE_OPT.to_be_bytes());
                out.extend_from_slice(&1232u16.to_be_bytes());
                out.extend_from_slice(&rr.rdata);
                out.extend_from_slice(&0u16.to_be_bytes());
                continue;
            }
            if rr.name == self.question.0 {
                // Compression pointer back to the question's name
                out.extend_from_slice(&[0xC0, HEADER_LEN as u8]);
            } else {
                encode_name(&rr.name, &mut out);
            }
            out.extend_from_slice(&rr.rtype.to_be_bytes());
            out.extend_from_slice(&CLASS_IN.to_be_bytes());
            out.extend_from_slice(&300u32.to_be_bytes());
            out.extend_from_slice(&(rr.rdata.len() as u16).to_be_bytes());
            out.extend_from_slice(&rr.rdata);
        }
        out
    }
}

pub fn test_parse_txt() {
    info!("test_parse_txt");
    let msg = ResponseBuilder::new(0x1111, "example.com", TYPE_TXT)
        .answer(Answer::txt("example.com", &[b"ab", b"cd"]))
        .answer(Answer::txt("other.example.com", &[b"chased"]))
        .answer(Answer::txt("example.com", &[&[0xff, 0xfe]]))
        .build();
    let resp = parse_response(&msg).expect("should parse");
    assert_eq!(resp.id, 0x1111);
    assert!(resp.qr);
    assert_eq!(resp.questions.len(), 1);
    assert_eq!(resp.questions[0].qtype, TYPE_TXT);
    // Multi-string records join, chased owners count, non-UTF-8 records drop
    assert_eq!(txt_records(&resp), vec!["abcd", "chased"]);
}

pub fn test_parse_ptr() {
    info!("test_parse_ptr");
    let msg = ResponseBuilder::new(0x2222, "4.3.2.1.in-addr.arpa", TYPE_PTR)
        .answer(Answer::ptr("4.3.2.1.in-addr.arpa", "host.example"))
        .build();
    let resp = parse_response(&msg).expect("should parse");
    assert_eq!(resp.answers.len(), 1);
    assert_eq!(
        resp.answers[0].rdata,
        RData::Ptr("host.example".to_string())
    );
}

pub fn test_parse_txt_rdata() {
    info!("test_parse_txt_rdata");
    assert_eq!(
        parse_txt(b"\x02ab\x01c").expect("should parse"),
        vec![b"ab".to_vec(), b"c".to_vec()]
    );
    assert!(parse_txt(b"\x05ab").is_err(), "string overrun");
}

pub fn test_classify() {
    info!("test_classify");
    let answered = ResponseBuilder::new(1, "example.com", TYPE_TXT)
        .answer(Answer::txt("example.com", &[b"x"]))
        .build();
    let parse = |msg: &[u8]| parse_response(msg).expect("should parse");
    assert!(classify(parse(&answered)).is_ok());

    let truncated = ResponseBuilder::new(1, "example.com", TYPE_TXT)
        .flags(0x8380)
        .answer(Answer::txt("example.com", &[b"x"]))
        .build();
    assert!(matches!(
        classify(parse(&truncated)),
        Err(DnsError::Truncated)
    ));

    let nxdomain = ResponseBuilder::new(1, "example.com", TYPE_TXT)
        .rcode(RCODE_NXDOMAIN)
        .build();
    assert!(matches!(
        classify(parse(&nxdomain)),
        Err(DnsError::NoRecords { rcode: 3 })
    ));

    let nodata = ResponseBuilder::new(1, "example.com", TYPE_TXT).build();
    assert!(matches!(
        classify(parse(&nodata)),
        Err(DnsError::NoRecords { rcode: 0 })
    ));

    let servfail = ResponseBuilder::new(1, "example.com", TYPE_TXT)
        .rcode(2)
        .build();
    assert!(matches!(
        classify(parse(&servfail)),
        Err(DnsError::Rcode(2))
    ));

    // BADVERS lives in the OPT record's extended rcode bits
    let badvers = ResponseBuilder::new(1, "example.com", TYPE_TXT)
        .rcode(0)
        .opt(0x0100_0000)
        .build();
    assert!(matches!(
        classify(parse(&badvers)),
        Err(DnsError::Rcode(16))
    ));
}

pub fn test_parse_malformed() {
    info!("test_parse_malformed");
    let good = ResponseBuilder::new(0x4444, "example.com", TYPE_TXT)
        .answer(Answer::txt("example.com", &[b"x"]))
        .build();

    assert!(
        parse_response(&good[..HEADER_LEN - 1]).is_err(),
        "short header"
    );
    assert!(
        parse_response(&good[..good.len() - 1]).is_err(),
        "truncated rdata"
    );

    // A pointer at the question's name that points at or past itself
    let mut forward = good.clone();
    forward[HEADER_LEN] = 0xC0;
    forward[HEADER_LEN + 1] = (HEADER_LEN + 4) as u8;
    assert!(parse_response(&forward).is_err(), "forward pointer");

    let mut selfref = good.clone();
    selfref[HEADER_LEN] = 0xC0;
    selfref[HEADER_LEN + 1] = HEADER_LEN as u8;
    assert!(parse_response(&selfref).is_err(), "self pointer");

    let mut bad_kind = good.clone();
    bad_kind[HEADER_LEN] = 0x40;
    assert!(parse_response(&bad_kind).is_err(), "reserved label type");

    // RDLENGTH claiming more bytes than the message holds
    let mut long_rdata = good.clone();
    let rdlen_pos = long_rdata.len() - 3 - 1;
    long_rdata[rdlen_pos] = 0xFF;
    assert!(parse_response(&long_rdata).is_err(), "rdlength overrun");

    // A TXT character-string longer than its rdata
    let mut long_txt = good.clone();
    let last = long_txt.len() - 2;
    long_txt[last] = 0x7F;
    assert!(parse_response(&long_txt).is_err(), "txt string overrun");
}

//////////////////////////////////////////////////////////////////////////////////

/// Runs only on a device, from `run_all_tests`: the platform system resolver and a real lookup
#[allow(dead_code)]
pub async fn test_platform_system_resolver() {
    info!("test_platform_system_resolver");
    let supports_txt = Resolver::supports_txt();
    info!("txt lookup supported: {}", supports_txt);

    let result = Resolver::txt_lookup("bootstrap-v1.veilid.net").await;
    if supports_txt {
        let records = result.expect("should resolve bootstrap txt");
        info!("device txt_lookup: {:?}", records);
        assert!(!records.is_empty(), "should return records");
    } else {
        // Android below API 29 has no system TXT resolver
        assert_eq!(result, Err(ResolverError::Unsupported));
    }

    let name = Resolver::ptr_lookup(IpAddr::V4(Ipv4Addr::new(9, 9, 9, 9)))
        .await
        .expect("should reverse-resolve");
    info!("device ptr_lookup: {}", name);
    assert!(!name.is_empty(), "should return a hostname");
}

#[expect(clippy::unused_async)]
pub async fn test_all() {
    test_parse_txt();
    test_parse_ptr();
    test_parse_txt_rdata();
    test_classify();
    test_parse_malformed();
}

/// Resolves against the real network; excluded from the offline unit test gate
#[allow(dead_code)]
pub async fn test_live() {
    info!("test_live");
    assert!(Resolver::supports_txt());
    let txt = Resolver::txt_lookup("bootstrap-v1.veilid.net")
        .await
        .expect("should resolve bootstrap txt");
    info!("txt_lookup: {:?}", txt);
    assert!(!txt.is_empty(), "should return records");
    assert!(
        txt.iter().all(|r| !r.is_empty()),
        "records should have text"
    );

    let nxdomain = Resolver::txt_lookup("no-such-host.veilid.net").await;
    info!("nxdomain: {:?}", nxdomain);
    assert!(matches!(nxdomain, Err(ResolverError::Generic(_))));

    let name = Resolver::ptr_lookup(IpAddr::V4(Ipv4Addr::new(9, 9, 9, 9)))
        .await
        .expect("should reverse-resolve");
    info!("ptr_lookup: {}", name);
    assert!(!name.is_empty(), "should return a hostname");

    let unnamed = Resolver::ptr_lookup(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1))).await;
    info!("unnamed: {:?}", unnamed);
    assert!(unnamed.is_err(), "documentation range has no PTR");
}
