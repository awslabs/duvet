use std::sync::Once;

pub fn init() {
    static TRACING: Once = Once::new();

    TRACING.call_once(|| {
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
    });
}
