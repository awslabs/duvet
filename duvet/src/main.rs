// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    duvet::tracing::init();

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
