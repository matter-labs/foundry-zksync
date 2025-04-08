use foundry_compilers::artifacts::EvmVersion;
use foundry_config::{Config, SolcReq};
use foundry_test_utils::{forgetest_async, util::OutputExt};
use foundry_zksync_compilers::compilers::zksolc::settings::BytecodeHash;
use semver::Version;

const COUNTER_SOURCE: &str = r#"
        // SPDX-License-Identifier: UNLICENSED
        pragma solidity ^0.8.20;

        contract Counter {
            uint256 public number;

            function increment() public {
                number++;
            }
        }
"#;
const COUNTER_ADDRESS: &str = "0xBa97Fa4C0fB1CB2dE3c4610A3206e50F4F96448C";

forgetest_async!(zk_verify_bytecode, |prj, cmd| {
    let config = Config {
        evm_version: EvmVersion::London,
        solc: Some(SolcReq::Version(Version::new(0, 8, 28))),
        zksync: foundry_config::zksync::ZkSyncConfig {
            hash_type: Some(BytecodeHash::Keccak256),
            zksolc: Some(SolcReq::Version(Version::new(1, 5, 11))),
            ..Default::default()
        },
        ..Default::default()
    };

    prj.add_source("Counter.sol", COUNTER_SOURCE).unwrap();

    prj.write_config(config);

    let args = vec![
        "verify-bytecode",
        COUNTER_ADDRESS,
        "Counter",
        "--rpc-url",
        "https://sepolia.era.zksync.dev",
        "--zksync",
        "--verifier-url",
        "https://block-explorer-api.sepolia.zksync.dev/api",
    ];

    let output = cmd.forge_fuse().args(args).assert_success().get_output().stdout_lossy();
    assert!(output.contains("Runtime code matched with status partial"));
});

forgetest_async!(zk_verify_bytecode_none, |prj, cmd| {
    let config = Config {
        evm_version: EvmVersion::London,
        solc: Some(SolcReq::Version(Version::new(0, 8, 28))),
        zksync: foundry_config::zksync::ZkSyncConfig {
            hash_type: Some(BytecodeHash::None),
            zksolc: Some(SolcReq::Version(Version::new(1, 5, 11))),
            ..Default::default()
        },
        ..Default::default()
    };

    prj.add_source("Counter.sol", COUNTER_SOURCE).unwrap();

    prj.write_config(config);

    let args = vec![
        "verify-bytecode",
        COUNTER_ADDRESS,
        "Counter",
        "--rpc-url",
        "https://sepolia.era.zksync.dev",
        "--zksync",
        "--verifier-url",
        "https://block-explorer-api.sepolia.zksync.dev/api",
    ];

    let output = cmd.forge_fuse().args(args).assert_success().get_output().stdout_lossy();
    assert!(output.contains("Runtime code matched with status partial"));
});

forgetest_async!(zk_verify_bytecode_ipfs, |prj, cmd| {
    let config = Config {
        evm_version: EvmVersion::London,
        solc: Some(SolcReq::Version(Version::new(0, 8, 28))),
        zksync: foundry_config::zksync::ZkSyncConfig {
            hash_type: Some(BytecodeHash::Ipfs),
            zksolc: Some(SolcReq::Version(Version::new(1, 5, 11))),
            ..Default::default()
        },
        ..Default::default()
    };

    prj.add_source("Counter.sol", COUNTER_SOURCE).unwrap();

    prj.write_config(config);

    let args = vec![
        "verify-bytecode",
        COUNTER_ADDRESS,
        "Counter",
        "--rpc-url",
        "https://sepolia.era.zksync.dev",
        "--zksync",
        "--verifier-url",
        "https://block-explorer-api.sepolia.zksync.dev/api",
    ];

    let output = cmd.forge_fuse().args(args).assert_success().get_output().stdout_lossy();
    assert!(output.contains("Runtime code matched with status partial"));
});

forgetest_async!(zk_verify_bytecode_error_diff, |prj, cmd| {
    let config = Config {
        evm_version: EvmVersion::Shanghai,
        solc: Some(SolcReq::Version(Version::new(0, 8, 26))),
        zksync: foundry_config::zksync::ZkSyncConfig {
            zksolc: Some(SolcReq::Version(Version::new(1, 5, 10))),
            optimizer: true,
            optimizer_mode: '1',
            ..Default::default()
        },
        ..Default::default()
    };

    prj.add_source("Counter.sol", COUNTER_SOURCE).unwrap();

    prj.write_config(config);

    let args = vec![
        "verify-bytecode",
        COUNTER_ADDRESS,
        "Counter",
        "--rpc-url",
        "https://sepolia.era.zksync.dev",
        "--zksync",
        "--verifier-url",
        "https://block-explorer-api.sepolia.zksync.dev/api",
    ];

    cmd.forge_fuse().args(args).assert_success().stderr_eq(
        r#"Error: Runtime code did not match - this may be due to varying compiler settings
EVM version mismatch: local=shanghai, onchain=london
Optimizer mode mismatch: local=1, onchain=3
"#,
    );
});

forgetest_async!(
    #[ignore = "Needs a ZKsync etherscan key"]
    zk_verify_bytecode_etherscan,
    |prj, cmd| {
        let config = Config {
            evm_version: EvmVersion::London,
            solc: Some(SolcReq::Version(Version::new(0, 8, 28))),
            zksync: foundry_config::zksync::ZkSyncConfig {
                hash_type: Some(BytecodeHash::Keccak256),
                zksolc: Some(SolcReq::Version(Version::new(1, 5, 11))),
                ..Default::default()
            },
            ..Default::default()
        };

        let etherscan_key = "TODO";

        prj.add_source("Counter.sol", COUNTER_SOURCE).unwrap();

        prj.write_config(config);

        let args = vec![
            "verify-bytecode",
            COUNTER_ADDRESS,
            "Counter",
            "--rpc-url",
            "https://sepolia.era.zksync.dev",
            "--zksync",
            "--verifier-url",
            "https://api-sepolia-era.zksync.network/api",
            "--etherscan-api-key",
            etherscan_key,
        ];

        let output = cmd.forge_fuse().args(args).assert_success().get_output().stdout_lossy();
        assert!(output.contains("Runtime code matched with status partial"));
    }
);
