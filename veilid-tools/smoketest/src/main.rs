#![deny(warnings)]

use veilid_tools::*;

#[cfg_attr(feature = "rt-wasm-bindgen", wasm_bindgen_test::wasm_bindgen_test)]
async fn smoke_test() {
    let jh = spawn("test", async {
        print!("Hello... ");
        sleep(1000).await;
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
