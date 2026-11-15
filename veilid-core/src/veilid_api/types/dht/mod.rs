mod allow_offline;
mod derivation;
mod dht_record_descriptor;
mod dht_record_report;
mod dht_transaction_set_value_options;
mod open_dht_record_options;
mod schema;
mod set_dht_value_options;
mod transact_dht_records_options;
mod value_data;
mod value_seq_num;
mod value_subkey_range_set;

use super::*;

pub use allow_offline::*;
pub(crate) use derivation::*;
pub use dht_record_descriptor::*;
pub use dht_record_report::*;
pub use dht_transaction_set_value_options::*;
pub use open_dht_record_options::*;
pub use schema::*;
pub use set_dht_value_options::*;
pub use transact_dht_records_options::*;
pub use value_data::*;
pub use value_seq_num::*;
pub use value_subkey_range_set::*;

/// Value subkey
#[cfg_attr(all(target_arch = "wasm32", target_os = "unknown"), declare)]
pub type ValueSubkey = u32;

/// Fixed length of MemberId (DHT Schema member id) in bytes
pub const MEMBER_ID_LENGTH: usize = 32;
/// The maximum size of a single subkey
pub const MAX_SUBKEY_SIZE: usize = 32768;
/// The maximum total size of all subkeys of a record
pub const MAX_RECORD_DATA_SIZE: usize = 1_048_576;
