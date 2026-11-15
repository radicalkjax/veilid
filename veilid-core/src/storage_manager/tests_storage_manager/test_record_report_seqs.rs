//! A report's seq lists follow its subkey range, so a per-subkey lookup must index by position
//! in that range - a counter over any subset of it names the wrong seq.

use super::*;

pub fn test_local_seq_for_follows_the_report_range() {
    let subkeys = ValueSubkeyRangeSet::new()
        .union(&ValueSubkeyRangeSet::single(0))
        .union(&ValueSubkeyRangeSet::single_range(3, 4));
    let local = vec![
        ValueSeqNum::from(7u32),
        ValueSeqNum::from(4u32),
        ValueSeqNum::NONE,
    ];
    let network = vec![
        ValueSeqNum::from(7u32),
        ValueSeqNum::from(5u32),
        ValueSeqNum::from(4u32),
    ];
    let report = DHTRecordReport::try_new(subkeys, ValueSubkeyRangeSet::new(), local, network)
        .expect("report should build");

    assert_eq!(report.local_seq_for(0), Some(ValueSeqNum::from(7u32)));
    assert_eq!(report.local_seq_for(3), Some(ValueSeqNum::from(4u32)));
    assert_eq!(report.local_seq_for(4), Some(ValueSeqNum::NONE));
    assert_eq!(report.local_seq_for(1), None, "not in the report");
    assert_eq!(report.local_seq_for(2), None, "not in the report");

    // The field case: newer subkeys {3, 4} walked with a counter compared subkey 3 against
    // subkey 0's seq (7) and skipped it, although the network held seq 5 > local 4
    let newer = report.newer_online_subkeys();
    assert_eq!(newer, ValueSubkeyRangeSet::single_range(3, 4));
    let first = newer.first().expect("newer is not empty");
    assert!(ValueSeqNum::from(5u32) > report.local_seq_for(first).expect("covered"));
}

pub fn test_all() {
    test_local_seq_for_follows_the_report_range();
}
