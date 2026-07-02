// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

#[cfg(not(target_family = "wasm"))]
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

// The native command-line binary. The wasm build ships as the `duvet-wasm`
// component crate instead, so the `duvet` binary is only built for native
// targets.
#[cfg(not(target_family = "wasm"))]
fn main() {
    let format = tracing_subscriber::fmt::format().compact(); // Use a less verbose output format.

    let env_filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(tracing::Level::ERROR.into())
        .with_env_var("DUVET_LOG")
        .from_env()
        .unwrap();

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .event_format(format)
        .with_test_writer()
        .init();

    let cache = duvet_core::Cache::default();
    let fs = duvet_core::vfs::fs::Fs::default();

    let runtime = tokio::runtime::Builder::new_current_thread()
        .on_thread_start({
            let cache = cache.clone();
            let fs = fs.clone();
            move || {
                cache.setup_thread();
                fs.setup_thread();
            }
        })
        .enable_all()
        // it usually takes longer to spawn threads than complete the program so keep the max low
        .max_blocking_threads(8)
        .build()
        .unwrap();

    runtime.block_on(async {
        if let Err(err) = duvet::run().await {
            eprintln!("{err:?}");
            std::process::exit(1);
        }
    });
}

// The native filesystem/multi-thread runtime is unavailable on wasm; the
// checks run through the `duvet-wasm` component instead of this binary.
#[cfg(target_family = "wasm")]
fn main() {
    panic!("the `duvet` binary is not supported on wasm; use the `duvet-wasm` component");
}
