// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use std::io::IsTerminal;

#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    // Install a miette report handler whose color output reflects the actual
    // destination, not whatever miette's default lazy autodetection sees.
    //
    // Diagnostics in duvet are commonly rendered via `format!("{:?}", error)`
    // inside Display impls (see duvet/src/query/result.rs). At that point the
    // formatter is a `core::fmt::Formatter` and miette's default handler has
    // no way to consult the eventual destination, so it ships ANSI escapes
    // unconditionally. Captured stdout/stderr in CI then contains color codes
    // that no one will see and that break snapshot tests.
    //
    // Honor `NO_COLOR` (https://no-color.org/) and disable color when stderr
    // is not a terminal. `set_hook` is fallible only if a hook is already set;
    // ignore the error so test harnesses that call into the duvet library
    // multiple times still work.
    let use_color = std::env::var_os("NO_COLOR").is_none() && std::io::stderr().is_terminal();
    let _ = miette::set_hook(Box::new(move |_| {
        Box::new(miette::MietteHandlerOpts::new().color(use_color).build())
    }));

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
