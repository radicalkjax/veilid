use super::*;
use crate::tests::*;

// An answer that fails validation fails its waiting question at once, not at the timeout
pub async fn test_invalid_answer_fails_its_question_at_once() {
    let (update_callback, config) = fixture_veilid_core();
    let api = api_startup(update_callback, config)
        .await
        .expect("startup failed");
    let registry = api.core_context().unwrap().registry();
    let rpc_processor = registry.rpc_processor();

    let op_id = OperationId::new(get_random_u64());
    let handle = rpc_processor.waiting_rpc_table.add_op_waiter(
        op_id,
        WaitingQuestion {
            answer_desc: "GetValueA",
            context: None,
        },
    );

    // A status answer to a question expecting a get value answer
    let answer = RPCOperation::new_answer(
        op_id,
        RPCAnswer::new(RPCAnswerDetail::StatusA(Box::new(
            RPCOperationStatusA::new(None, None, None),
        ))),
        SenderPeerInfo::new_no_peer_info(Timestamp::now()),
    );
    let res = rpc_processor.validate_rpc_operation(&answer).await;
    assert!(matches!(res, Err(RPCError::Protocol(_))), "{:?}", res);

    let timeout = TimestampDuration::new_ms(
        rpc_processor
            .config()
            .internal()
            .network
            .rpc
            .timeout_ms
            .into(),
    );
    let waited = rpc_processor
        .waiting_rpc_table
        .wait_for_op(handle, timeout)
        .await
        .expect("wait should not error");
    let TimeoutOr::Value((Err(e), _)) = waited else {
        panic!("question was not failed");
    };
    assert!(matches!(e, RPCError::Protocol(_)), "{:?}", e);

    api.shutdown().await;
}
