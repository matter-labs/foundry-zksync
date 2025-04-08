//! The `forge` CLI: build, test, fuzz, debug and deploy Solidity contracts, like Hardhat, Brownie,
//! Ape.
use forge::args::run;
use foundry_cli::utils;
use foundry_common::{POSTHOG_API_KEY, TELEMETRY_CONFIG_NAME};
use zksync_telemetry::init_telemetry;

#[cfg(all(feature = "jemalloc", unix))]
#[global_allocator]
static ALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

fn main() {
    let _ = utils::block_on(init_telemetry(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        TELEMETRY_CONFIG_NAME,
        Some(POSTHOG_API_KEY.into()),
        None,
        None,
    ));
    if let Err(err) = run() {
        let _ = foundry_common::sh_err!("{err:?}");
        std::process::exit(1);
    }
}
