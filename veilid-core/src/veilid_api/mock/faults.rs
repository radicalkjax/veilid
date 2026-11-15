//! Injected failures, so a test can drive the paths a real network reaches on its
//! own and a mock never does.
//!
//! The mock succeeds at everything, which makes it useless for the failures that
//! hurt: a transaction that cannot reach consensus in begin, a subkey an
//! inspect reports and a read cannot produce. Those are recovery paths with
//! arbitration and adoption logic behind them, and nothing else exercises them.
//!
//! Armed through the debug channel rather than an API of its own, so the switch
//! exists only where the mock does: `fault arm`, `fault list`, `fault clear`.

use super::*;

/// An operation a fault can be attached to.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum MockFaultOp {
    Begin,
    Commit,
    Get,
    Set,
    Inspect,
    Open,
    Create,
}

impl MockFaultOp {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "begin" => Self::Begin,
            "commit" => Self::Commit,
            "get" => Self::Get,
            "set" => Self::Set,
            "inspect" => Self::Inspect,
            "open" => Self::Open,
            "create" => Self::Create,
            _ => return None,
        })
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Begin => "begin",
            Self::Commit => "commit",
            Self::Get => "get",
            Self::Set => "set",
            Self::Inspect => "inspect",
            Self::Open => "open",
            Self::Create => "create",
        }
    }
}

/// How a faulted operation goes wrong.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum MockFaultMode {
    /// What a node that could not reach consensus returns, which every caller retries
    TryAgain,
    /// The operation times out
    Timeout,
    /// The record is not found
    KeyNotFound,
    /// A generic failure
    Generic,
    /// Succeed and produce nothing, the partial read a reader must defer on
    Silent,
    /// The network holds a different value at the stored sequence number, as a split write settles
    Rival,
}

impl MockFaultMode {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "try_again" => Self::TryAgain,
            "timeout" => Self::Timeout,
            "key_not_found" => Self::KeyNotFound,
            "generic" => Self::Generic,
            "silent" => Self::Silent,
            "rival" => Self::Rival,
            _ => return None,
        })
    }

    fn name(&self) -> &'static str {
        match self {
            Self::TryAgain => "try_again",
            Self::Timeout => "timeout",
            Self::KeyNotFound => "key_not_found",
            Self::Generic => "generic",
            Self::Silent => "silent",
            Self::Rival => "rival",
        }
    }
}

struct MockFault {
    /// Operation the fault attaches to
    op: MockFaultOp,
    /// How the operation fails
    mode: MockFaultMode,
    /// Firings left, or `None` to keep firing until cleared
    remaining: Option<u32>,
    /// Only records whose key or opaque key starts with this
    record: Option<String>,
    /// Only subkeys in this set, which is how a partial read is staged
    subkeys: Option<Vec<ValueSubkey>>,
    /// Firings so far
    fired: u32,
    /// Bytes a rival carries, or `None` for the stored bytes inverted
    data: Option<Vec<u8>>,
}

/// What an operation does once [check_fault] lets it run.
pub(crate) enum MockFaultAction {
    /// Run as normal
    Proceed,
    /// Succeed and produce nothing
    Silent,
    /// Plant a rival on the network, then run as normal
    Rival(Option<Vec<u8>>),
}

static MOCK_FAULTS: LazyLock<Mutex<Vec<MockFault>>> = LazyLock::new(|| Mutex::new(Vec::new()));

impl MockFault {
    fn matches(
        &self,
        op: MockFaultOp,
        record_key: Option<&RecordKey>,
        subkey: Option<ValueSubkey>,
    ) -> bool {
        if self.op != op {
            return false;
        }
        if let Some(prefix) = &self.record {
            let Some(record_key) = record_key else {
                return false;
            };
            if !record_key.to_string().starts_with(prefix.as_str())
                && !record_key.opaque().to_string().starts_with(prefix.as_str())
            {
                return false;
            }
        }
        if let Some(subkeys) = &self.subkeys {
            let Some(subkey) = subkey else {
                return false;
            };
            if !subkeys.contains(&subkey) {
                return false;
            }
        }
        true
    }
}

/// The fault armed for this operation, if any, counting the firing against it.
fn take_fault(
    op: MockFaultOp,
    record_key: Option<&RecordKey>,
    subkey: Option<ValueSubkey>,
) -> Option<(MockFaultMode, Option<Vec<u8>>)> {
    let mut faults = MOCK_FAULTS.lock();
    let index = faults
        .iter()
        .position(|f| f.matches(op, record_key, subkey))?;
    let mode = faults[index].mode;
    let data = faults[index].data.clone();
    faults[index].fired += 1;
    if let Some(remaining) = &mut faults[index].remaining {
        *remaining -= 1;
        if *remaining == 0 {
            faults.remove(index);
        }
    }
    Some((mode, data))
}

/// Fail an operation the way its armed fault says, or say how it runs instead.
pub(crate) fn check_fault(
    op: MockFaultOp,
    record_key: Option<&RecordKey>,
    subkey: Option<ValueSubkey>,
) -> VeilidAPIResult<MockFaultAction> {
    let Some((mode, data)) = take_fault(op, record_key, subkey) else {
        return Ok(MockFaultAction::Proceed);
    };
    match mode {
        MockFaultMode::Silent => Ok(MockFaultAction::Silent),
        MockFaultMode::Rival => Ok(MockFaultAction::Rival(data)),
        MockFaultMode::TryAgain => Err(VeilidAPIError::try_again(format!(
            "injected fault: {} could not reach consensus",
            op.name()
        ))),
        MockFaultMode::Timeout => Err(VeilidAPIError::timeout()),
        MockFaultMode::KeyNotFound => Err(match record_key {
            Some(record_key) => VeilidAPIError::key_not_found(record_key.opaque()),
            None => VeilidAPIError::generic("injected fault: key not found"),
        }),
        MockFaultMode::Generic => Err(VeilidAPIError::generic(format!(
            "injected fault on {}",
            op.name()
        ))),
    }
}

/// One optional `--name value` or `--name=value` argument, consumed if present.
///
/// The long-flag form the real record commands use for their refinements
/// (`record purge local --keys ...`), rather than a `name=value` shape that
/// appears nowhere else in the command set.
fn take_flag<'a>(args: &mut Vec<&'a str>, name: &str) -> VeilidAPIResult<Option<&'a str>> {
    let flag = format!("--{}", name);
    let prefix = format!("--{}=", name);
    for i in 0..args.len() {
        if let Some(value) = args[i].strip_prefix(prefix.as_str()) {
            args.remove(i);
            return Ok(Some(value));
        }
        if args[i] == flag {
            if i + 1 >= args.len() {
                apibail_missing_argument!("flag needs a value", flag);
            }
            args.remove(i);
            return Ok(Some(args.remove(i)));
        }
    }
    Ok(None)
}

/// Bytes from a debug argument: `#` and hex, or the text itself.
fn parse_data(text: &str) -> VeilidAPIResult<Vec<u8>> {
    match text.strip_prefix('#') {
        Some(hex_text) => hex::decode(hex_text)
            .map_err(|e| VeilidAPIError::invalid_argument(e.to_string(), "data", text)),
        None => Ok(text.as_bytes().to_vec()),
    }
}

/// The `fault` debug subcommands: `arm`, `list` and `clear`.
///
/// Deliberately not `set`: that is one of the operations a fault attaches to,
/// and `fault set set silent` reads as a typo rather than as a command.
pub(crate) fn mock_debug_fault(words: &[&str]) -> VeilidAPIResult<String> {
    match words {
        ["clear"] => {
            let count = {
                let mut faults = MOCK_FAULTS.lock();
                let count = faults.len();
                faults.clear();
                count
            };
            Ok(format!("cleared {} faults", count))
        }
        ["list"] => {
            let faults = MOCK_FAULTS.lock();
            if faults.is_empty() {
                return Ok("no faults armed".to_owned());
            }
            Ok(faults
                .iter()
                .map(|f| {
                    format!(
                        "{} {} remaining={} record={} subkeys={} fired={}",
                        f.op.name(),
                        f.mode.name(),
                        f.remaining
                            .map(|r| r.to_string())
                            .unwrap_or_else(|| "forever".to_owned()),
                        f.record.as_deref().unwrap_or("*"),
                        f.subkeys
                            .as_ref()
                            .map(|s| s
                                .iter()
                                .map(|k| k.to_string())
                                .collect::<Vec<_>>()
                                .join(","))
                            .unwrap_or_else(|| "*".to_owned()),
                        f.fired
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"))
        }
        ["arm", op, mode, rest @ ..] => {
            let Some(op) = MockFaultOp::parse(op) else {
                apibail_invalid_argument!("unknown fault op", "op", op);
            };
            let Some(mode) = MockFaultMode::parse(mode) else {
                apibail_invalid_argument!("unknown fault mode", "mode", mode);
            };
            // Only a read or a write has a way to succeed at nothing
            if mode == MockFaultMode::Silent && !matches!(op, MockFaultOp::Get | MockFaultOp::Set) {
                apibail_invalid_argument!(
                    "silent applies to get and set only",
                    "mode",
                    mode.name()
                );
            }
            if mode == MockFaultMode::Rival && op != MockFaultOp::Get {
                apibail_invalid_argument!("rival applies to get only", "mode", mode.name());
            }

            let mut args: Vec<&str> = rest.to_vec();
            let remaining = match take_flag(&mut args, "count")? {
                None => None,
                Some(count) => Some(count.parse::<u32>().map_err(|e| {
                    VeilidAPIError::invalid_argument(e.to_string(), "count", count)
                })?),
            };
            let record = take_flag(&mut args, "record")?.map(|r| r.to_owned());
            let subkeys = match take_flag(&mut args, "subkeys")? {
                None => None,
                Some(list) => Some(
                    list.split(',')
                        .map(|k| {
                            k.parse::<ValueSubkey>().map_err(|e| {
                                VeilidAPIError::invalid_argument(e.to_string(), "subkeys", k)
                            })
                        })
                        .collect::<VeilidAPIResult<Vec<_>>>()?,
                ),
            };
            let data = match take_flag(&mut args, "data")? {
                None => None,
                Some(text) if mode == MockFaultMode::Rival => Some(parse_data(text)?),
                Some(text) => apibail_invalid_argument!("data applies to rival only", "data", text),
            };
            if let Some(unknown) = args.first() {
                apibail_invalid_argument!("unknown fault argument", "args", unknown);
            }

            MOCK_FAULTS.lock().push(MockFault {
                op,
                mode,
                remaining,
                record,
                subkeys,
                fired: 0,
                data,
            });
            Ok(format!("armed {} {}", op.name(), mode.name()))
        }
        _ => Err(VeilidAPIError::invalid_argument(
            "bad fault command",
            "args",
            words.join(" "),
        )),
    }
}
