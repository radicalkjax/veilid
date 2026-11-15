#![expect(
    clippy::disallowed_macros,
    reason = "a build script: cargo:rustc-link-lib and friends are its protocol"
)]
// allow, not expect: only the geolocation feature reads the wall clock
#![allow(
    clippy::disallowed_methods,
    reason = "compares file mtimes against the wall clock, not node time"
)]

use sha2::{Digest, Sha256};
use std::fs::OpenOptions;
use std::io::BufRead;
use std::io::Write;
use std::{
    io,
    path::Path,
    process::{Command, Stdio},
};

const CAPNP_VERSION: &str = "1.1.0";

fn get_desired_capnp_version_string() -> String {
    CAPNP_VERSION.to_string()
}

fn get_capnp_version_string() -> String {
    let output = Command::new("capnp")
        .arg("--version")
        .stdout(Stdio::piped())
        .output()
        .expect("capnp was not in the PATH, and is required for the build when you have changed any .capnp files");
    let s = String::from_utf8(output.stdout)
        .expect("'capnp --version' output was not a valid string")
        .trim()
        .to_owned();

    if !s.starts_with("Cap'n Proto version ") {
        panic!("invalid capnp version string: {}", s);
    }
    s[20..].to_owned()
}

fn is_input_file_outdated<P, Q>(input: P, output: Q) -> io::Result<bool>
where
    P: AsRef<Path>,
    Q: AsRef<Path>,
{
    let (Some(out_bh), Some(out_capnp_hash)) = get_build_hash_and_capnp_version_hash(output) else {
        // output file not found or no build hash, we are outdated
        println!("cargo:warning=Output file not found or no build hash.");
        return Ok(true);
    };

    // Check if desired CAPNP_VERSION hash has changed
    let mut hasher = Sha256::new();
    hasher.update(get_desired_capnp_version_string().as_bytes());
    let capnp_hash = hasher.finalize().to_vec();

    if !std::fs::exists(&input).unwrap() {
        println!("cargo:warning=Capnp file is missing, rebuilding.");
        return Ok(true);
    }

    let in_bh = make_build_hash(input);

    if out_bh != in_bh {
        println!("cargo:warning=Build hash has changed.");
        return Ok(true);
    }

    if out_capnp_hash != capnp_hash {
        println!("cargo:warning=Capnp desired version hash has changed.");
        return Ok(true);
    }

    Ok(false)
}

fn calculate_hash(lines: std::io::Lines<std::io::BufReader<std::fs::File>>) -> Vec<u8> {
    let mut hasher = Sha256::new();
    // Build hash of lines, ignoring EOL conventions
    for l in lines {
        let l = l.unwrap();
        hasher.update(l.as_bytes());
        hasher.update(b"\n");
    }
    let out = hasher.finalize();
    out.to_vec()
}

fn get_build_hash_and_capnp_version_hash<Q: AsRef<Path>>(
    output_path: Q,
) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    let output_file = match std::fs::File::open(output_path).ok() {
        // Returns a file handle if the file exists
        Some(f) => f,
        // Returns None, None if the file does not exist
        None => return (None, None),
    };
    let lines = std::io::BufReader::new(output_file).lines();
    let mut build_hash = None;
    let mut capnp_version_hash = None;
    for l in lines {
        let l = l.unwrap();
        if let Some(rest) = l.strip_prefix("//BUILDHASH:") {
            build_hash = Some(hex::decode(rest).unwrap());
        } else if let Some(rest) = l.strip_prefix("//CAPNPDESIREDVERSIONHASH:") {
            capnp_version_hash = Some(hex::decode(rest).unwrap());
        }
    }
    (build_hash, capnp_version_hash)
}

fn make_build_hash<P: AsRef<Path>>(input_path: P) -> Vec<u8> {
    let input_path = input_path.as_ref();
    let lines = std::io::BufReader::new(std::fs::File::open(input_path).unwrap()).lines();
    calculate_hash(lines)
}

fn append_hash_and_desired_capnp_version_hash<P: AsRef<Path>, Q: AsRef<Path>>(
    input_path: P,
    output_path: Q,
) {
    let input_path = input_path.as_ref();
    let output_path = output_path.as_ref();
    let lines = std::io::BufReader::new(std::fs::File::open(input_path).unwrap()).lines();
    let h = calculate_hash(lines);

    let mut out_file = OpenOptions::new().append(true).open(output_path).unwrap();
    writeln!(out_file, "\n//BUILDHASH:{}", hex::encode(h)).unwrap();

    let mut hasher = Sha256::new();
    hasher.update(get_desired_capnp_version_string().as_bytes());
    writeln!(
        out_file,
        "\n//CAPNPDESIREDVERSIONHASH:{}",
        hex::encode(hasher.finalize())
    )
    .unwrap();
}

fn do_capnp_build() {
    let desired_capnp_version_string = get_desired_capnp_version_string();
    let capnp_version_string = get_capnp_version_string();

    // Check capnp version
    let desired_capnp_major_version = desired_capnp_version_string
        .split_once('.')
        .unwrap()
        .0
        .parse::<usize>()
        .expect("should be valid int");

    if capnp_version_string
        .split_once('.')
        .unwrap()
        .0
        .parse::<usize>()
        .expect("should be valid int")
        != desired_capnp_major_version
    {
        panic!(
            "capnproto version should be major version 1, preferably {} but is {}",
            desired_capnp_version_string, capnp_version_string
        );
    } else if capnp_version_string != desired_capnp_version_string {
        println!(
            "cargo:warning=capnproto version may be untested: {}",
            capnp_version_string
        );
    }

    ::capnpc::CompilerCommand::new()
        .file("proto/veilid.capnp")
        .output_path(".")
        .run()
        .expect("compiling schema");

    // If successful, append a hash of the input to the output file
    // Also append a hash of the desired capnp version to the output file
    append_hash_and_desired_capnp_version_hash("proto/veilid.capnp", "proto/veilid_capnp.rs");
}

/// The MaxMind EULA requires the data be refreshed regularly, so this is a live fetch and not a
/// vendored blob. Source: https://github.com/sapics/ip-location-db
#[cfg(feature = "geolocation")]
mod geoip {
    use std::path::Path;
    use std::time::{Duration, SystemTime};

    type Error = Box<dyn std::error::Error>;

    const SOURCES: [(&str, &str); 2] = [
        (
            "https://cdn.jsdelivr.net/npm/@ip-location-db/asn-country-mmdb/asn-country-ipv4.mmdb",
            "ipv4.mmdb",
        ),
        (
            "https://cdn.jsdelivr.net/npm/@ip-location-db/asn-country-mmdb/asn-country-ipv6.mmdb",
            "ipv6.mmdb",
        ),
    ];

    const MAX_AGE: Duration = Duration::from_secs(60 * 60 * 24);
    const FETCH_TIMEOUT: Duration = Duration::from_secs(120);
    // the databases are ~1MB and ~2MB today; the cap only has to catch a runaway response
    const MAX_SIZE: u64 = 64 * 1024 * 1024;
    const MIN_SIZE: usize = 64 * 1024;
    // every MaxMind DB file ends with this marker followed by the metadata section
    const METADATA_MARKER: &[u8] = b"\xab\xcd\xefMaxMind.com";

    fn download(url: &str) -> Result<Vec<u8>, Error> {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(FETCH_TIMEOUT))
            .build()
            .new_agent();
        let mut res = agent.get(url).call()?;
        let body = res.body_mut().with_config().limit(MAX_SIZE).read_to_vec()?;
        validate(&body)?;
        Ok(body)
    }

    /// A CDN error page is a 200 with an HTML body; embedding one yields a reader that answers
    /// None for every address with nothing in the log to say why
    fn validate(body: &[u8]) -> Result<(), Error> {
        if body.len() < MIN_SIZE {
            return Err(format!(
                "response too small to be a maxmind database: {} bytes",
                body.len()
            )
            .into());
        }
        let tail = &body[body.len().saturating_sub(128 * 1024)..];
        if !tail
            .windows(METADATA_MARKER.len())
            .any(|w| w == METADATA_MARKER)
        {
            return Err("response has no maxmind metadata marker".into());
        }
        Ok(())
    }

    fn is_fresh(path: &Path) -> bool {
        path.metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age <= MAX_AGE)
    }

    fn refresh(url: &str, path: &Path) -> Result<(), Error> {
        if is_fresh(path) {
            return Ok(());
        }
        match download(url) {
            Ok(body) => {
                // Atomic publish: a truncated file with a fresh mtime would stay in place for a day
                let tmp = path.with_extension("mmdb.part");
                std::fs::write(&tmp, &body)?;
                std::fs::rename(&tmp, path)?;
                Ok(())
            }
            Err(e) if path.exists() => {
                println!(
                    "cargo:warning=geoip: {url} unreachable ({e}), using stale {}",
                    path.display()
                );
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    pub fn download_database_files() -> Result<(), Error> {
        let target_dir =
            std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?).join("../target");
        std::fs::create_dir_all(&target_dir)?;
        for (url, name) in SOURCES {
            let path = target_dir.join(name);
            println!("cargo:rerun-if-changed={}", path.display());
            refresh(url, &path)?;
        }
        Ok(())
    }
}

fn main() {
    println!("cargo:rustc-check-cfg=cfg(build_docs,values(none()))");

    if std::env::var("DOCS_RS").is_ok()
        || std::env::var("CARGO_CFG_DOC").is_ok()
        || std::env::var("BUILD_DOCS").is_ok()
    {
        println!("cargo:rustc-cfg=build_docs");
        return;
    }

    // Gather build-time information, such as the features that were enabled when veilid-core was built.
    bosion::gather();

    println!(
        "cargo:rustc-env=TARGET={}",
        std::env::var("TARGET").unwrap()
    );

    if is_input_file_outdated("./proto/veilid.capnp", "./proto/veilid_capnp.rs").unwrap() {
        println!("cargo:warning=rebuilding proto/veilid_capnp.rs because it has changed from the last generation of proto/veilid.capnp");
        do_capnp_build();
    }

    #[cfg(feature = "geolocation")]
    geoip::download_database_files().expect("failed to download geoip database files");
}
