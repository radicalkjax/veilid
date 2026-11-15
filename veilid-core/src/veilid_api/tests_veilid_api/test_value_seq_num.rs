use super::*;

pub fn value_seq_num_next() {
    assert_eq!(ValueSeqNum::NONE.next().unwrap(), ValueSeqNum::ZERO);
    assert_eq!(ValueSeqNum::ZERO.next().unwrap(), ValueSeqNum::from(1));
    assert!(ValueSeqNum::MAX.next().is_err());
}

pub fn value_seq_num_saturating_next() {
    assert_eq!(ValueSeqNum::NONE.saturating_next(), ValueSeqNum::ZERO);
    assert_eq!(
        ValueSeqNum::from(u32::MAX - 2).saturating_next(),
        ValueSeqNum::MAX
    );
    assert_eq!(ValueSeqNum::MAX.saturating_next(), ValueSeqNum::MAX);
}

pub fn value_seq_num_ordering() {
    // "never written" sorts below the first write, so max() picks a real seq over NONE
    assert!(ValueSeqNum::NONE < ValueSeqNum::ZERO);
    assert!(ValueSeqNum::ZERO < ValueSeqNum::MAX);
    assert_eq!(ValueSeqNum::ZERO.max(ValueSeqNum::NONE), ValueSeqNum::ZERO);
}

#[cfg(all(feature = "debug-api", not(feature = "mock-api")))]
pub fn value_seq_num_parse() {
    use crate::veilid_api::debug::parse_value_seq_num as parse;

    assert_eq!(parse("max").unwrap(), ValueSeqNum::MAX);
    assert_eq!(parse("MAX").unwrap(), ValueSeqNum::MAX);
    assert_eq!(parse("max-0").unwrap(), ValueSeqNum::MAX);
    assert_eq!(
        parse("max-1").unwrap(),
        ValueSeqNum::from(u32::MAX - 2),
        "max-1 must leave exactly one write"
    );
    assert_eq!(parse("0").unwrap(), ValueSeqNum::ZERO);
    assert_eq!(parse("42").unwrap(), ValueSeqNum::from(42));

    assert!(parse("max-4294967295").is_err());
    assert!(parse("max+1").is_err());
    assert!(parse("maximum").is_err());
    assert!(parse("").is_err());
}
