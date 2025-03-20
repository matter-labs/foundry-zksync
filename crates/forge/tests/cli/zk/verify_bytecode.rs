use foundry_compilers::artifacts::EvmVersion;
use foundry_config::{Config, SolcReq};
use foundry_test_utils::{
    forgetest_async,
    rpc::{next_http_archive_rpc_url, next_mainnet_etherscan_api_key},
    util::OutputExt,
    TestCommand, TestProject,
};
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
        optimizer: Some(true),
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
        optimizer: Some(true),
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
        optimizer: Some(true),
        solc: Some(SolcReq::Version(Version::new(0, 8, 28))),
        zksync: foundry_config::zksync::ZkSyncConfig {
            hash_type: Some(BytecodeHash::Ipfs),
            zksolc: Some(SolcReq::Version(Version::new(1, 5, 11))),
            ..Default::default()
        },
        ..Default::default()
    };

    prj.add_source(
        "Counter.sol",
        r#"
        // SPDX-License-Identifier: UNLICENSED
        pragma solidity ^0.8.20;

        contract Counter {
            uint256 public number;

            function increment() public {
                number++;
            }
        }
    "#,
    )
    .unwrap();

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

// forgetest_async!(can_verify_bytecode_no_metadata, |prj, cmd| {
//     test_verify_bytecode(
//         prj,
//         cmd,
//         "0xba2492e52F45651B60B8B38d4Ea5E2390C64Ffb1",
//         "SystemConfig",
//         None,
//         Config {
//             evm_version: EvmVersion::London,
//             optimizer_runs: Some(999999),
//             optimizer: Some(true),
//             cbor_metadata: false,
//             bytecode_hash: BytecodeHash::None,
//             ..Default::default()
//         },
//         "etherscan",
//         "https://api.etherscan.io/api",
//         ("partial", "partial"),
//     );
// });
//
// forgetest_async!(can_verify_bytecode_with_metadata, |prj, cmd| {
//     test_verify_bytecode(
//         prj,
//         cmd,
//         "0xb8901acb165ed027e32754e0ffe830802919727f",
//         "L1_ETH_Bridge",
//         None,
//         Config {
//             evm_version: EvmVersion::Paris,
//             optimizer_runs: Some(50000),
//             optimizer: Some(true),
//             ..Default::default()
//         },
//         "etherscan",
//         "https://api.etherscan.io/api",
//         ("partial", "partial"),
//     );
// });
