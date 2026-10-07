use std::collections::HashMap;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args();
    let prog = args.next().unwrap_or_else(|| "emit_schema".to_string());
    let Some(name) = args.next() else {
        eprintln!("usage: {prog} <Request|RecvMessage>");
        return ExitCode::from(2);
    };

    let mut schemas = HashMap::<String, String>::new();
    veilid_remote_api::emit_schemas(&mut schemas);

    if let Some(schema) = schemas.get(&name) {
        println!("{schema}");
        ExitCode::SUCCESS
    } else {
        let mut valid: Vec<_> = schemas.keys().cloned().collect();
        valid.sort();
        eprintln!("unknown schema '{name}'; valid: {}", valid.join(", "));
        ExitCode::from(1)
    }
}
