use duvet::report::Report;
use std::path::PathBuf;

#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Debug)]
enum Arguments {
    Extract(Extract),
    Report(Report),
}

#[derive(Debug)]
struct Extract {
    manifest_path: PathBuf,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    duvet_core::testing::init_tracing();

    let manifest_path = std::env::current_dir().unwrap().join("duvet.toml");

    Report { manifest_path }.run().await.unwrap()
}
