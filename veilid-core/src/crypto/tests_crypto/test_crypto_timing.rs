//! Wall time of the synchronous crypto operations behind the async crypto guard, so
//! the choice between running an operation inline and offloading it rests on a
//! measurement rather than a guess. Prints, never asserts: the numbers move with
//! the build profile and the host.

use super::*;

/// Runs at least `min_iters` and then keeps going until 200ms have elapsed, so a
/// millisecond-resolution clock (wasm: `Date.now()`) still resolves microsecond ops
fn time_us<R>(min_iters: u32, mut f: impl FnMut() -> R) -> f64 {
    let floor = TimestampDuration::new_ms(200);
    let start = Timestamp::now();
    let mut iters = 0u32;
    loop {
        core::hint::black_box(f());
        iters += 1;
        let elapsed = Timestamp::now().duration_since(start);
        if iters >= min_iters && elapsed >= floor {
            return elapsed.as_u64() as f64 / iters as f64;
        }
    }
}

fn report(name: &str, us: f64) {
    info!("crypto op timing: {:<44} {:>10.1} us", name, us);
}

pub fn test_op_timings(cs: &CryptoSystemGuard<'_>) {
    let kp = cs.generate_keypair();
    let kp2 = cs.generate_keypair();
    let kinds = [
        ("32B", 32usize),
        ("1KB", 1024),
        ("8KB", 8192),
        ("64KB", 65536),
    ];

    report("random_nonce", time_us(1000, || cs.random_nonce()));
    report("random_bytes(32)", time_us(1000, || cs.random_bytes(32)));
    report(
        "random_shared_secret",
        time_us(1000, || cs.random_shared_secret()),
    );
    report(
        "cached_dh (hit)",
        time_us(1000, || cs.cached_dh(&kp2.key(), &kp.secret())),
    );
    report(
        "compute_dh (miss)",
        time_us(200, || cs.compute_dh(&kp2.key(), &kp.secret())),
    );
    report("generate_keypair", time_us(200, || cs.generate_keypair()));
    report(
        "validate_keypair",
        time_us(200, || cs.validate_keypair(&kp.key(), &kp.secret())),
    );
    for (label, len) in kinds {
        let data = cs.random_bytes(len);
        report(
            &format!("generate_hash({})", label),
            time_us(200, || cs.generate_hash(&data)),
        );
        let hash = cs.generate_hash(&data);
        report(
            &format!("validate_hash({})", label),
            time_us(200, || cs.validate_hash(&data, &hash)),
        );
        report(
            &format!("sign({})", label),
            time_us(200, || cs.sign(&kp.key(), &kp.secret(), &data)),
        );
        let sig = cs.sign(&kp.key(), &kp.secret(), &data).unwrap();
        report(
            &format!("verify({})", label),
            time_us(200, || cs.verify(&kp.key(), &data, &sig)),
        );
        let secret = cs.random_shared_secret();
        let nonce = cs.random_nonce();
        report(
            &format!("encrypt_aead({})", label),
            time_us(200, || cs.encrypt_aead(&data, &nonce, &secret, None)),
        );
        let sealed = cs.encrypt_aead(&data, &nonce, &secret, None).unwrap();
        report(
            &format!("decrypt_aead({})", label),
            time_us(200, || cs.decrypt_aead(&sealed, &nonce, &secret, None)),
        );
        report(
            &format!("crypt_no_auth_unaligned({})", label),
            time_us(200, || cs.crypt_no_auth_unaligned(&data, &nonce, &secret)),
        );
    }
    report(
        "generate_kem_keypair",
        time_us(50, || cs.generate_kem_keypair()),
    );
    report(
        "encapsulation_key_from_signing_key",
        time_us(200, || cs.encapsulation_key_from_signing_key(&kp.key())),
    );
    report(
        "decapsulation_key_from_signing_secret",
        time_us(200, || {
            cs.decapsulation_key_from_signing_secret(&kp.secret())
        }),
    );
    // Control: the optimized crate's cipher alone, no cryptosystem wrapper, same 64KB
    {
        let mut buf = cs.random_bytes(65536);
        let key = [7u8; 32];
        let nonce = [9u8; 24];
        report(
            "xchacha20_apply_keystream(64KB)",
            time_us(200, || {
                veilid_core_optimized::xchacha20_apply_keystream(&key, &nonce, &mut buf)
            }),
        );
    }
    let salt = cs.random_bytes(cs.default_salt_length());
    report(
        "derive_shared_secret (argon2)",
        time_us(1, || cs.derive_shared_secret(b"password", &salt)),
    );
}

pub async fn test_all() {
    let api = crypto_tests_startup().await;
    {
        let crypto = api.crypto().unwrap();
        for v in VALID_CRYPTO_KINDS {
            let cs = crypto.get(v).unwrap();
            info!("crypto op timing for {}", v);
            test_op_timings(&cs);
        }
    }
    crypto_tests_shutdown(api.clone()).await;
    assert!(api.is_shutdown());
}
