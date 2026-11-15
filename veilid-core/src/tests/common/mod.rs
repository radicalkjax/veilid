pub mod test_attachment_manager;
// Two native nodes in one process; wasm has no UDP or TCP to point one at the other
#[cfg(all(
    not(feature = "mock-api"),
    not(all(target_arch = "wasm32", target_os = "unknown"))
))]
pub mod test_bootstrap;
pub mod test_dht;
pub mod test_dht_transactions;

use super::*;
