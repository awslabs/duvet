// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

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
        .build()
        .unwrap();

    runtime.block_on(async {
        if let Err(err) = duvet::run().await {
            eprintln!("{err:?}");
            std::process::exit(1);
        }
    });
}
