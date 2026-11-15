use super::*;

/// Where the host application stands in its lifecycle, as the host reports it.
#[apply(api_data_enum!)]
#[api(eq, copy, ts(namespace))]
pub enum VeilidAppLifecycleState {
    /// In front of the user, or doing work its platform keeps it awake for.
    Active,
    /// Put away: its platform may freeze or terminate it without further notice.
    Background,
}
