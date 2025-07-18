//! Contains various tests for checking forge commands related to verifying contracts on Etherscan
//! and ZkSync explorer.

use crate::utils::{self, network_private_key, EnvExternalities};
use alloy_chains::NamedChain;
use foundry_common::retry::Retry;
use foundry_test_utils::{
    forgetest,
    util::{OutputExt, TestCommand, TestProject},
};
use std::time::Duration;

/// Adds a `Unique` contract to the source directory of the project that can be imported as
/// `import {Unique} from "./unique.sol";`
fn add_unique(prj: &TestProject) {
    let timestamp = utils::millis_since_epoch();
    prj.add_source(
        "unique",
        &format!(
            r#"
contract Unique {{
    uint public _timestamp = {timestamp};
}}
"#
        ),
    )
    .unwrap();
}

fn add_verify_target(prj: &TestProject) {
    prj.add_source(
        "Verify.sol",
        r#"
import {Unique} from "./unique.sol";
contract Verify is Unique {
function doStuff() external {}
}
"#,
    )
    .unwrap();
}

fn add_single_verify_target_file(prj: &TestProject) {
    let timestamp = utils::millis_since_epoch();
    let contract = format!(
        r#"
contract Unique {{
    uint public _timestamp = {timestamp};
}}
contract Verify is Unique {{
function doStuff() external {{}}
}}
"#
    );

    prj.add_source("Verify.sol", &contract).unwrap();
}

#[allow(clippy::disallowed_macros)]
fn parse_verification_result(cmd: &mut TestCommand, retries: u32) -> eyre::Result<()> {
    // Give Block Explorer some time to verify the contract.
    Retry::new(retries, Duration::from_secs(30)).run(|| -> eyre::Result<()> {
        let output = cmd.execute();
        let out = String::from_utf8_lossy(&output.stdout);
        if out.contains("Verification was successful") {
            return Ok(());
        }
        eyre::bail!(
            "Failed to get verification, stdout: {}, stderr: {}",
            out,
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn verify_check(guid: String, chain: String, mut cmd: TestCommand) {
    let args = vec![
        "verify-check",
        &guid,
        "--chain-id",
        &chain,
        "--verifier-url",
        "https://explorer.sepolia.era.zksync.dev/contract_verification",
        "--verifier",
        "zksync",
    ];

    cmd.forge_fuse().args(args);

    parse_verification_result(&mut cmd, 6).expect("Failed to verify check")
}

fn await_verification_response(info: EnvExternalities, mut cmd: TestCommand) {
    let guid = {
        Retry::new(5, Duration::from_secs(60))
            .run(|| -> eyre::Result<String> {
                let output = cmd.execute();
                let out = String::from_utf8_lossy(&output.stdout);
                parse_verification_id(&out).ok_or_else(|| {
                    eyre::eyre!(
                        "Failed to get guid, stdout: {}, stderr: {}",
                        out,
                        String::from_utf8_lossy(&output.stderr)
                    )
                })
            })
            .expect("Failed to get verify guid")
    };

    verify_check(guid, info.chain.to_string(), cmd);
}

fn deploy_contract(
    info: &EnvExternalities,
    contract_path: &str,
    prj: TestProject,
    cmd: &mut TestCommand,
) -> String {
    add_unique(&prj);
    add_verify_target(&prj);
    let output = cmd
        .forge_fuse()
        .arg("create")
        .args(create_args(info))
        .arg(contract_path)
        .arg("--zksync")
        .assert_success()
        .get_output()
        .stdout_lossy();
    utils::parse_deployed_address(output.as_str())
        .unwrap_or_else(|| panic!("Failed to parse deployer {output}"))
}

#[allow(clippy::disallowed_macros)]
fn verify_on_chain(info: Option<EnvExternalities>, prj: TestProject, mut cmd: TestCommand) {
    // only execute if keys present
    if let Some(info) = info {
        println!("verifying on {}", info.chain);

        let contract_path = "src/Verify.sol:Verify";
        let address = deploy_contract(&info, contract_path, prj, &mut cmd);

        let args = vec![
            "--chain-id".to_string(),
            info.chain.to_string(),
            address,
            contract_path.to_string(),
            "--zksync".to_string(),
            "--verifier-url".to_string(),
            "https://explorer.sepolia.era.zksync.dev/contract_verification".to_string(),
            "--verifier".to_string(),
            "zksync".to_string(),
        ];

        cmd.forge_fuse().arg("verify-contract").root_arg().args(args);

        await_verification_response(info, cmd)
    }
}

/// Executes create --verify on the given chain
#[allow(clippy::disallowed_macros)]
fn create_verify_on_chain(info: Option<EnvExternalities>, prj: TestProject, mut cmd: TestCommand) {
    // only execute if keys present
    if let Some(info) = info {
        println!("verifying on {}", info.chain);
        add_single_verify_target_file(&prj);

        let contract_path = "src/Verify.sol:Verify";
        let output = cmd
            .arg("create")
            .args(create_args(&info))
            .args([contract_path, "--verify"])
            .args([
                "--verifier-url".to_string(),
                "https://explorer.sepolia.era.zksync.dev/contract_verification".to_string(),
                "--verifier".to_string(),
                "zksync".to_string(),
                "--zksync".to_string(),
            ])
            .assert_success()
            .get_output()
            .stdout_lossy();

        assert!(output.contains("Verification was successful"), "{}", output);
    }
}

fn create_args(ext: &EnvExternalities) -> Vec<String> {
    vec![
        "--chain".to_string(),
        ext.chain.to_string(),
        "--private-key".to_string(),
        ext.pk.clone(),
        "--rpc-url".to_string(),
        "https://sepolia.era.zksync.dev".to_string(),
    ]
}

fn zk_env_externalities() -> Option<EnvExternalities> {
    Some(EnvExternalities {
        chain: NamedChain::ZkSyncTestnet,
        rpc: String::new(),
        pk: network_private_key("zksync_testnet")?,
        etherscan: String::new(),
        verifier: "zksync".to_string(),
    })
}

fn parse_verification_id(out: &str) -> Option<String> {
    for line in out.lines() {
        if line.contains("Verification submitted successfully. Verification ID: ") {
            return Some(
                line.replace("Verification submitted successfully. Verification ID: ", "")
                    .replace('`', "")
                    .trim()
                    .to_string(),
            );
        }
    }
    None
}

// tests `create && contract-verify && verify-check` on Sepolia testnet if correct env vars are set
// ZKSYNC_TESTNET_TEST_PRIVATE_KEY=0x...
forgetest!(zk_can_verify_random_contract_sepolia, |prj, cmd| {
    verify_on_chain(zk_env_externalities(), prj, cmd);
});

// tests `create --verify on Sepolia testnet if correct env vars are set
forgetest!(zk_can_create_verify_random_contract_sepolia, |prj, cmd| {
    create_verify_on_chain(zk_env_externalities(), prj, cmd);
});
