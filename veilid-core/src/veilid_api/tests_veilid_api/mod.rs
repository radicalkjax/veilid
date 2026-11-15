pub mod mocks;
pub mod test_attachment_condition;
pub mod test_config;
pub mod test_serialize_json;
pub mod test_value_data;
pub mod test_value_seq_num;

pub use mocks::*;

use super::*;

#[expect(clippy::unused_async)]
pub async fn test_all() {
    test_serialize_json::test_serialize_json();
    test_attachment_condition::attachment_state_ordering();
    test_attachment_condition::attachment_condition_satisfied_by();
    test_value_data::value_data_ok();
    test_value_data::value_data_too_long();
    test_value_seq_num::value_seq_num_next();
    test_value_seq_num::value_seq_num_saturating_next();
    test_value_seq_num::value_seq_num_ordering();
    #[cfg(all(feature = "debug-api", not(feature = "mock-api")))]
    test_value_seq_num::value_seq_num_parse();
    test_config::test_internal_footgun_gate();
    test_config::test_bind_retry_ms_defaults_when_absent();
}
