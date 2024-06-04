//! Test helpers for Forge integration tests.

use alloy_primitives::U256;
use foundry_compilers::{
    artifacts::{Libraries, Settings},
    zksync::compile::output::ProjectCompileOutput as ZkProjectCompileOutput,
    Project, ProjectCompileOutput, ProjectPathsConfig, SolcConfig,
};
use foundry_config::Config;
use foundry_evm::{
    constants::CALLER,
    executors::{Executor, FuzzedExecutor},
    opts::{Env, EvmOpts},
    revm::db::DatabaseRef,
};
use foundry_test_utils::fd_lock;
use once_cell::sync::Lazy;
use std::{env, io::Write};

pub const RE_PATH_SEPARATOR: &str = "/";

pub const TESTDATA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata");

pub static PROJECT: Lazy<Project> = Lazy::new(|| {
    let paths = ProjectPathsConfig::builder().root(TESTDATA).sources(TESTDATA).build().unwrap();

    let libs =
        ["fork/Fork.t.sol:DssExecLib:0xfD88CeE74f7D78697775aBDAE53f9Da1559728E4".to_string()];
    let settings = Settings { libraries: Libraries::parse(&libs).unwrap(), ..Default::default() };
    let solc_config = SolcConfig::builder().settings(settings).build();

    Project::builder().paths(paths).solc_config(solc_config).build().unwrap()
});

pub static COMPILED: Lazy<ProjectCompileOutput> = Lazy::new(|| {
    const LOCK: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/.lock");

    let project = &*PROJECT;
    assert!(project.cached);

    // Compile only once per test run.
    // We need to use a file lock because `cargo-nextest` runs tests in different processes.
    // This is similar to [`foundry_test_utils::util::initialize`], see its comments for more
    // details.
    let mut lock = fd_lock::new_lock(LOCK);
    let read = lock.read().unwrap();
    let out;
    if project.cache_path().exists() && std::fs::read(LOCK).unwrap() == b"1" {
        out = project.compile();
        drop(read);
    } else {
        drop(read);
        let mut write = lock.write().unwrap();
        write.write_all(b"1").unwrap();
        out = project.compile();
        drop(write);
    };

    let out = out.unwrap();
    if out.has_compiler_errors() {
        panic!("Compiled with errors:\n{out}");
    }
    out
});

/// Compile ZK project
fn zk_compile(project: Project) -> ZkProjectCompileOutput {
    // let compiler_path =
    //     futures::executor::block_on(setup_zksolc_manager(DEFAULT_ZKSOLC_VERSION.to_owned()))
    //         .expect("failed setting up zksolc");

    // let mut zksolc_config = ZkSolcConfigBuilder::new()
    //     .compiler_path(compiler_path)
    //     .settings(ZkSettings {
    //         optimizer: Optimizer {
    //             enabled: Some(true),
    //             mode: Some(String::from("3")),
    //             fallback_to_optimizing_for_size: Some(false),
    //             disable_system_request_memoization: true,
    //             ..Default::default()
    //         },
    //         ..Default::default()
    //     })
    //     .build()
    //     .expect("failed building zksolc config");
    // zksolc_config.contracts_to_compile = Some(vec![
    //     globset::Glob::new("zk/*").unwrap().compile_matcher(),
    //     globset::Glob::new("lib/*").unwrap().compile_matcher(),
    //     globset::Glob::new("cheats/Vm.sol").unwrap().compile_matcher(),
    // ]);

    // let mut zksolc = ZkSolc::new(zksolc_config, project);
    // let (zk_out, _) = zksolc.compile().unwrap();
    // zk_out

    project.zksync_compile().expect("failed compiling with zksolc")
}

pub static COMPILED_ZK: Lazy<ZkProjectCompileOutput> = Lazy::new(|| {
    const LOCK: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/.lock-zk");

    // let project = &*PROJECT;
    let mut paths = ProjectPathsConfig::builder().root(TESTDATA).sources(TESTDATA).build().unwrap();
    paths.zksync_artifacts = format!("{TESTDATA}/zkout").into();

    let libs =
        ["fork/Fork.t.sol:DssExecLib:0xfD88CeE74f7D78697775aBDAE53f9Da1559728E4".to_string()];
    let settings = Settings { libraries: Libraries::parse(&libs).unwrap(), ..Default::default() };
    let solc_config = SolcConfig::builder().settings(settings).build();

    let project = Project::builder().paths(paths).solc_config(solc_config).build().unwrap();
    assert!(project.cached);

    // Compile only once per test run.
    // We need to use a file lock because `cargo-nextest` runs tests in different processes.
    // This is similar to [`foundry_test_utils::util::initialize`], see its comments for more
    // details.
    let mut lock = fd_lock::new_lock(LOCK);
    let read = lock.read().unwrap();
    let out;
    if project.cache_path().exists() && std::fs::read(LOCK).unwrap() == b"1" {
        out = zk_compile(project);
        drop(read);
    } else {
        drop(read);
        let mut write = lock.write().unwrap();
        write.write_all(b"1").unwrap();
        out = zk_compile(project);
        drop(write);
    };

    if out.has_compiler_errors() {
        panic!("Compiled with errors:\n{out}");
    }
    out
});

pub static EVM_OPTS: Lazy<EvmOpts> = Lazy::new(|| EvmOpts {
    env: Env {
        gas_limit: u64::MAX,
        chain_id: None,
        tx_origin: Config::DEFAULT_SENDER,
        block_number: 1,
        block_timestamp: 1,
        ..Default::default()
    },
    sender: Config::DEFAULT_SENDER,
    initial_balance: U256::MAX,
    ffi: true,
    verbosity: 3,
    memory_limit: 1 << 26,
    ..Default::default()
});

pub fn fuzz_executor<DB: DatabaseRef>(executor: Executor) -> FuzzedExecutor {
    let cfg = proptest::test_runner::Config { failure_persistence: None, ..Default::default() };

    FuzzedExecutor::new(
        executor,
        proptest::test_runner::TestRunner::new(cfg),
        CALLER,
        crate::config::test_opts().fuzz,
    )
}
