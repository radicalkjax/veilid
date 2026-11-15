use super::*;

impl VeilidAPI {
    /// The debug commands a mocked node supports.
    ///
    /// The real command set reaches into subsystems mock-api compiles out, so only the commands
    /// tests drive are served here. Anything else says so instead of returning nothing.
    pub async fn debug(&self, args: String) -> VeilidAPIResult<String> {
        let args = args.trim();
        let words: Vec<&str> = args.split_whitespace().collect();

        match words.as_slice() {
            [] | ["help"] => Ok(MOCK_DEBUG_HELP.to_owned()),
            // Purging the local record store leaves the records on their storage nodes
            ["record", "purge", "local"] => {
                let count = MockNetwork::with_node(&self.mock_key()?, |node| {
                    let count = node.local_records.len();
                    node.opened_records.clear();
                    node.local_records.clear();
                    node.local_descriptors.clear();
                    node.offline_writes.clear();
                    count
                })?;
                Ok(format!("purged {} local records", count))
            }
            // A mock node stores nothing for anyone else, so its remote record store is empty
            ["record", "purge", "remote"] => Ok("purged 0 remote records".to_owned()),
            ["record", "list"] => {
                let keys = with_records(|records| {
                    records.keys().map(|k| k.to_string()).collect::<Vec<_>>()
                });
                Ok(format!("{} records:\n{}", keys.len(), keys.join("\n")))
            }
            ["record", "list", "local"] => {
                let keys = MockNetwork::with_node(&self.mock_key()?, |node| {
                    node.local_records
                        .keys()
                        .map(|k| k.to_string())
                        .collect::<Vec<_>>()
                })?;
                Ok(format!(
                    "{} local records:\n{}",
                    keys.len(),
                    keys.join("\n")
                ))
            }
            ["nodeid"] => {
                let node_ids = MockNetwork::with_node(&self.mock_key()?, |node| {
                    node.node_ids
                        .iter()
                        .map(|id| id.to_string())
                        .collect::<Vec<_>>()
                })?;
                Ok(node_ids.join(" "))
            }
            // Same grammar as the real command: `config [insecure] [key]`
            ["config", rest @ ..] => {
                let config = self.config()?;
                let (config, rest) = match rest {
                    ["insecure", rest @ ..] => (config, rest),
                    rest => (config.safe(), rest),
                };
                // Materialize the effective internal config so `internal.*` keys
                // resolve even when startup json omitted the section
                let mut config = (*config).clone();
                if config.internal.is_none() {
                    let effective = config.internal().clone();
                    config.internal = Some(effective);
                }
                match rest {
                    [] => config.get_key_json("", true),
                    [key] => config.get_key_json(key, true),
                    _ => Err(VeilidAPIError::invalid_argument(
                        "debug_config",
                        "args",
                        args.to_owned(),
                    )),
                }
            }
            ["fault", rest @ ..] => mock_debug_fault(rest),
            _ => Ok(format!(
                "unsupported under mock-api: '{}'\n\n{}",
                args, MOCK_DEBUG_HELP
            )),
        }
    }
}

const MOCK_DEBUG_HELP: &str = "\
mock-api debug commands:
    record purge local      close every record and drop this node's copies
    record purge remote     no-op: a mock node holds no records for others
    record list             list the mock network's records
    record list local       list the records this node holds copies of
    nodeid                  show this node's ids
    config [insecure] [key] dump config json, or one dot-path key
    fault arm OP MODE [--count N] [--record PREFIX] [--subkeys A,B] [--data D]
                            fail an operation the way the network would.
                            OP is what to fail:
                              begin commit get set inspect open create
                            MODE is how it fails:
                              try_again timeout key_not_found generic silent
                              rival
                            silent means succeed and produce nothing, and
                            applies to get and set only.
                            rival applies to get only: before the get runs,
                            the network's value becomes a different one at
                            the same seq, unannounced, so only a read that
                            reaches the network sees it. --data gives its
                            bytes (text, or # and hex); by default it is the
                            stored bytes inverted.
                            --count limits how many times it fires, default
                            until cleared; --record and --subkeys narrow it
                            to one record or a few of its subkeys.
    fault list              what is armed, and how often each has fired
    fault clear             disarm every fault";
