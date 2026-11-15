//! Peer blame under our own loss: lost questions and failed sends are charged at their blame weight,
//! so a run of them needs 1/weight times as many, an answer starts the count over, and only a
//! full-weight loss ends the run of steady answers.

use super::*;
use crate::network_manager::{AddressType, ProtocolType, TransportType};
use crate::tests::mock_registry;

pub async fn test_charges_follow_the_blame_weight() {
    info!("--- test_charges_follow_the_blame_weight ---");
    let registry = mock_registry::init("weighted_blame").await;
    {
        let routing_table = registry.routing_table();
        let node = seed_nodes(&routing_table, 1).remove(0);
        let udp = TransportType::new(ProtocolType::UDP, AddressType::IPV4);
        let lost = |weight: f64| node.operate_mut(|e| e.lost_question(udp, true, weight));
        let failed = |weight: f64| {
            node.operate_mut(|e| e.failed_to_send(Timestamp::now(), true, udp, true, weight))
        };
        let lost_count = || node.operate(|e| e.rpc_stats().recent_lost_questions);
        let failed_count = || node.operate(|e| e.rpc_stats().failed_to_send);
        let answered = || {
            let ts = Timestamp::now();
            node.operate_mut(|e| e.answer_rcvd(ts, ts, ByteCount::new(0), udp, true));
        };

        // Full weight charges every loss
        lost(1.0);
        assert_eq!(lost_count(), 1);

        // At a quarter, four losses make one charge
        for _ in 0..3 {
            lost(0.25);
        }
        assert_eq!(lost_count(), 1);
        lost(0.25);
        assert_eq!(lost_count(), 2);

        // An answer ends the run, the uncharged fraction with it
        answered();
        assert_eq!(lost_count(), 0);
        for _ in 0..3 {
            lost(0.25);
        }
        answered();
        lost(0.25);
        assert_eq!(lost_count(), 0, "fractions do not carry across an answer");

        // Only a full-weight loss ends the run of steady answers that paces pings
        let steady = || {
            node.operate(|e| {
                (
                    e.rpc_stats().first_steady_answer_ts,
                    e.per_sequence_ordering_stats()
                        .get(&udp.sequence_ordering())
                        .and_then(|so| so.first_steady_answer_ts),
                )
            })
        };
        answered();
        let run = steady();
        assert!(run.0.is_some() && run.1.is_some());
        lost(0.5);
        lost(0.5);
        assert_eq!(lost_count(), 1);
        assert_eq!(steady(), run, "a weighted charge keeps the steady run");
        lost(1.0);
        assert_eq!(steady(), (None, None));
        answered();

        // Weight zero never charges
        for _ in 0..100 {
            lost(0.0);
        }
        assert_eq!(lost_count(), 0);

        // Failed sends the same way, and a successful send clears them
        for _ in 0..3 {
            failed(0.5);
        }
        assert_eq!(failed_count(), 1);
        node.operate_mut(|e| e.question_sent(Timestamp::now(), ByteCount::new(0), true, udp, true));
        assert_eq!(failed_count(), 0);
        failed(0.5);
        assert_eq!(failed_count(), 0, "a send clears the uncharged fraction");
        failed(0.5);
        assert_eq!(failed_count(), 1);
    }
    mock_registry::terminate(registry).await;
}
