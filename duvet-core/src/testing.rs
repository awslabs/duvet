// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

pub fn init_tracing() {
    use std::sync::Once;

    static TRACING: Once = Once::new();

    // make sure this only gets initialized once
    TRACING.call_once(|| {
        let format = tracing_subscriber::fmt::format()
            //.with_level(false) // don't include levels in formatted output
            //.with_ansi(false)
            .compact(); // Use a less verbose output format.

        let env_filter = tracing_subscriber::EnvFilter::builder()
            .with_default_directive(tracing::Level::DEBUG.into())
            .with_env_var("DUVET_LOG")
            .from_env()
            .unwrap();

        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .event_format(format)
            .with_test_writer()
            .init();
    });
}
