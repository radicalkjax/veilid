#![deny(warnings)]

use std::sync::Arc;
use veilid_core::tools::{sleep, spawn};
use veilid_core::*;

#[cfg_attr(feature = "rt-wasm-bindgen", wasm_bindgen_test::wasm_bindgen_test)]
async fn smoke_test() {
    let jh = spawn("test", async {
        print!("Hello... ");

        // Create a basic update callback to display Veilid's update events
        let update_callback = Arc::new(move |update: VeilidUpdate| {
            match update {
                _ => {
                    println!("{:#?}", update)
                }
            };
        });

        // Set up a config for this application
        let exe_dir = std::env::current_exe()
            .map(|x| x.parent().map(|p| p.to_owned()))
            .ok()
            .flatten()
            .unwrap_or(".".into());
        let config = VeilidConfig {
            program_name: "veilid-core smoketest".into(),
            namespace: "veilid-core-smoketest".into(),
            protected_store: VeilidConfigProtectedStore {
                // IMPORTANT: don't do this in production
                // This avoids prompting for a password and is insecure
                always_use_insecure_storage: true,
                directory: exe_dir
                    .join(".veilid/protected_store")
                    .to_string_lossy()
                    .to_string(),
                ..Default::default()
            },
            table_store: VeilidConfigTableStore {
                directory: exe_dir
                    .join(".veilid/table_store")
                    .to_string_lossy()
                    .to_string(),
                ..Default::default()
            },
            ..Default::default()
        };

        // Startup Veilid node
        let veilid = veilid_core::api_startup(update_callback, config)
            .await
            .expect("Failed to startup Veilid node");

        // Wait a little
        sleep(1000).await;

        // Shutdown without attach
        veilid.shutdown().await;

        println!("world!");
    });
    jh.await;
}

fn main() {
    #[cfg(feature = "rt-tokio")]
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(smoke_test());
    #[cfg(feature = "rt-async-std")]
    async_std::task::block_on(smoke_test());
}
