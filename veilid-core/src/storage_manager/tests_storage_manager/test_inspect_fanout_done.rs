//! An inspect stops early only once SOME subkey has reached consensus. A range in
//! which no subkey is ever answered can therefore never stop early, and runs to
//! `max_find_node_count` nodes or the full `get_value_timeout_ms`. Callers that
//! choose an inspect range depend on this, so it is pinned here.

use super::*;
use inspect_record::{descriptor_fanout_done, inspect_fanout_done};

const CONSENSUS: usize = 3;

fn done(counts: &[usize], stalled: bool) -> bool {
    matches!(
        inspect_fanout_done(counts.iter().copied(), CONSENSUS, true, stalled),
        FanoutDoneDisposition::DoneEarly
    )
}

/// One written subkey at consensus is enough to stop, however many empties ride along.
fn test_stops_with_one_answered_subkey() {
    assert!(done(&[CONSENSUS], true));
    // The E:0 case: subkey 0 answered, the rest of the range unwritten
    assert!(done(&[CONSENSUS, 0, 0, 0, 0, 0, 0, 0], true));
}

/// A range where nothing is written can never stop early, at any length.
fn test_all_empty_range_never_stops() {
    for len in [1, 2, 16, 256] {
        assert!(
            !done(&vec![0; len], true),
            "a range of {len} unwritten subkeys must not stop early, at any length"
        );
    }
}

/// A subkey still gathering answers holds the fanout open, so a partial result is
/// never mistaken for a settled one.
fn test_converging_subkey_holds_it_open() {
    for partial in 1..CONSENSUS {
        assert!(
            !done(&[CONSENSUS, partial], true),
            "a subkey with {partial} of {CONSENSUS} answers is still converging"
        );
    }
    // Once it catches up, the fanout may stop
    assert!(done(&[CONSENSUS, CONSENSUS], true));
}

/// A round that added something is not a stopping point, even at full consensus:
/// answers still arriving could raise a seq.
fn test_progress_holds_it_open() {
    assert!(!done(&[CONSENSUS], false));
    assert!(done(&[CONSENSUS], true));
}

/// No subkeys requested at all cannot stop either: an empty subkey set reaches the
/// wire as the whole record and matches no answer, so a caller holding nothing
/// locally must not inspect rather than inspect an empty range.
fn test_empty_request_never_stops() {
    assert!(!done(&[], true));
}

/// Without a descriptor there is nothing to validate answers against.
fn test_missing_descriptor_holds_it_open() {
    assert!(!matches!(
        inspect_fanout_done([CONSENSUS], CONSENSUS, false, true),
        FanoutDoneDisposition::DoneEarly
    ));
}

/// A descriptor hashes to the record key, so one node settles it and no count of further
/// nodes can. This is what keeps opening a record whose subkeys have never been written
/// off the timeout, and what makes an open a single round trip.
fn test_descriptor_stops_at_the_first_node() {
    assert!(matches!(
        descriptor_fanout_done(true, true),
        FanoutDoneDisposition::DoneEarly
    ));
}

/// No descriptor is nothing to open the record with.
fn test_descriptor_missing_holds_it_open() {
    assert!(!matches!(
        descriptor_fanout_done(false, true),
        FanoutDoneDisposition::DoneEarly
    ));
}

/// A descriptor arriving without seqs leaves a fanout result list that does not line up
/// with the subkeys the descriptor names, so it is not a stopping point.
fn test_descriptor_without_seqs_holds_it_open() {
    assert!(!matches!(
        descriptor_fanout_done(true, false),
        FanoutDoneDisposition::DoneEarly
    ));
}

pub fn test_all() {
    test_stops_with_one_answered_subkey();
    test_all_empty_range_never_stops();
    test_converging_subkey_holds_it_open();
    test_progress_holds_it_open();
    test_empty_request_never_stops();
    test_missing_descriptor_holds_it_open();
    test_descriptor_stops_at_the_first_node();
    test_descriptor_missing_holds_it_open();
    test_descriptor_without_seqs_holds_it_open();
}
