// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

fn main() {
    let cache = duvet_core::Cache::default();
    let fs = duvet_core::vfs::fs::Fs::default();

    let runtime = tokio::runtime::Builder::new_multi_thread()
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
        if let Err(err) = duvet::arguments().await.exec().await {
            eprintln!("{}", err);
            std::process::exit(1);
        }
    });
}
