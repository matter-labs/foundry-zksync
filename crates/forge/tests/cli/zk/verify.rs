use crate::utils::{self, EnvExternalities, network_private_key};
use alloy_chains::NamedChain;
use alloy_dyn_abi::DynSolValue;
use alloy_primitives::{Address, U256, hex};
use foundry_common::retry::Retry;
use foundry_test_utils::{
    forgetest,
    util::{OutputExt, TestCommand, TestProject},
};
use std::{str::FromStr, time::Duration};

const ZKSYNC_VERIFIER_URL: &str = "https://explorer.sepolia.era.zksync.dev/contract_verification";
const VERIFY_INDEXING_WAIT_SECS: u64 = 15;

fn encode_constructor_args_hex(value: u64, name: &str, owner: &str) -> String {
    let owner = Address::from_str(owner).expect("invalid owner address");
    let init_data = DynSolValue::Tuple(vec![
        DynSolValue::Uint(U256::from(value), 256),
        DynSolValue::String(name.to_string()),
    ]);
    let encoded = DynSolValue::Tuple(vec![init_data, DynSolValue::Address(owner)]).abi_encode();
    format!("0x{}", hex::encode(encoded))
}

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
    );
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
    );
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

    prj.add_source("Verify.sol", &contract);
}

fn add_single_verify_target_with_constructor_file(prj: &TestProject) {
    let timestamp = utils::millis_since_epoch();
    let contract = format!(
        r#"
contract Unique {{
    uint public _timestamp = {timestamp};
}}
contract Verify is Unique {{
    struct InitData {{
        uint256 value;
        string name;
    }}
    constructor(InitData memory data, address owner) {{}}
    function doStuff() external {{}}
}}
"#
    );

    prj.add_source("Verify.sol", &contract);
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
        ZKSYNC_VERIFIER_URL,
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
        .arg("--broadcast")
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
            ZKSYNC_VERIFIER_URL.to_string(),
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
        let output_o = cmd
            .arg("create")
            .args(create_args(&info))
            .args([contract_path, "--verify"])
            .arg("--broadcast")
            .args([
                "--verifier-url".to_string(),
                ZKSYNC_VERIFIER_URL.to_string(),
                "--verifier".to_string(),
                "zksync".to_string(),
                "--zksync".to_string(),
            ])
            .execute();

        assert!(
            output_o.status.success(),
            "create --verify failed: {}",
            String::from_utf8_lossy(&output_o.stderr)
        );
        let stdout = String::from_utf8_lossy(&output_o.stdout);
        let stderr = String::from_utf8_lossy(&output_o.stderr);
        let merged = format!("{stdout}\n{stderr}");

        if merged.contains("Verification was successful") {
            // done
        } else if let Some(guid) = parse_verification_id(&merged) {
            verify_check(guid, info.chain.to_string(), cmd);
        } else if let Some(address) = utils::parse_deployed_address(&merged) {
            // Fallback: submit verification explicitly, then poll
            let args = vec![
                "--chain-id".to_string(),
                info.chain.to_string(),
                address,
                contract_path.to_string(),
                "--zksync".to_string(),
                "--verifier-url".to_string(),
                ZKSYNC_VERIFIER_URL.to_string(),
                "--verifier".to_string(),
                "zksync".to_string(),
            ];

            cmd.forge_fuse().arg("verify-contract").root_arg().args(args);
            await_verification_response(info, cmd)
        } else {
            panic!("{}", merged);
        }
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
    // Fallback to generic GUID parser used elsewhere in tests
    utils::parse_verification_guid(out)
}

// tests `create && contract-verify && verify-check` on Sepolia testnet if correct env vars are set
// ZKSYNC_TESTNET_PRIVATE_KEY=0x... (run with --test-threads=1 to avoid nonce conflicts)
forgetest!(test_zk_verify_random_contract_sepolia, |prj, cmd| {
    verify_on_chain(zk_env_externalities(), prj, cmd);
});

// tests `create --verify` on Sepolia testnet if correct env vars are set
// ZKSYNC_TESTNET_PRIVATE_KEY=0x... (run with --test-threads=1 to avoid nonce conflicts)
forgetest!(test_zk_create_verify_random_contract_sepolia, |prj, cmd| {
    create_verify_on_chain(zk_env_externalities(), prj, cmd);
});

/// Executes create --verify with optimization runs on the given chain
#[allow(clippy::disallowed_macros)]
fn create_verify_with_optimization_runs(
    info: Option<EnvExternalities>,
    prj: TestProject,
    mut cmd: TestCommand,
) {
    // only execute if keys present
    if let Some(info) = info {
        add_single_verify_target_file(&prj);

        let contract_path = "src/Verify.sol:Verify";
        let output = cmd
            .arg("create")
            .args(create_args(&info))
            .args([contract_path, "--verify"])
            .arg("--broadcast")
            .args([
                "--verifier-url".to_string(),
                ZKSYNC_VERIFIER_URL.to_string(),
                "--verifier".to_string(),
                "zksync".to_string(),
                "--zksync".to_string(),
                "--optimizer-runs".to_string(),
                "200".to_string(),
            ])
            .assert_success()
            .get_output()
            .stdout_lossy();

        assert!(output.contains("Verification was successful"), "{}", output);
    }
}

// tests `create --verify` with optimization runs on Sepolia testnet if correct env vars are set
// ZKSYNC_TESTNET_PRIVATE_KEY=0x... (run with --test-threads=1 to avoid nonce conflicts)
forgetest!(test_zk_create_verify_with_optimization_runs_sepolia, |prj, cmd| {
    create_verify_with_optimization_runs(zk_env_externalities(), prj, cmd);
});

fn deploy_with_constructor_args(
    info: Option<EnvExternalities>,
    prj: TestProject,
    mut cmd: TestCommand,
) {
    if let Some(info) = info {
        add_single_verify_target_with_constructor_file(&prj);

        let contract_path = "src/Verify.sol:Verify";
        let output = cmd
            .arg("create")
            .args(create_args(&info))
            .arg(contract_path)
            .arg("--zksync")
            .arg("--broadcast")
            .arg("--constructor-args")
            .arg("(42,TestString)")
            .arg("0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045")
            .assert_success()
            .get_output()
            .stdout_lossy();

        assert!(output.contains("Compiler run successful!"));
        assert!(output.contains("Deployed to:"));
        assert!(output.contains("Transaction hash:"));
    }
}

fn create_then_verify_with_constructor_args(
    info: Option<EnvExternalities>,
    prj: TestProject,
    mut cmd: TestCommand,
) {
    if let Some(info) = info {
        add_single_verify_target_with_constructor_file(&prj);

        let contract_path = "src/Verify.sol:Verify";

        let deploy_result = cmd
            .arg("create")
            .args(create_args(&info))
            .arg(contract_path)
            .arg("--zksync")
            .arg("--broadcast")
            .arg("--constructor-args")
            .arg("(42,TestString)")
            .arg("0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045")
            .execute();

        let deploy_stdout = String::from_utf8_lossy(&deploy_result.stdout);

        assert!(deploy_result.status.success(), "Deployment should succeed");
        assert!(deploy_stdout.contains("Compiler run successful!"));
        assert!(deploy_stdout.contains("Deployed to:"));

        let deployed_address = utils::parse_deployed_address(&deploy_stdout)
            .unwrap_or_else(|| panic!("Could not find deployed contract address in output"));

        std::thread::sleep(Duration::from_secs(VERIFY_INDEXING_WAIT_SECS));

        let verify_result = cmd
            .forge_fuse()
            .arg("verify-contract")
            .root_arg()
            .arg("--chain-id")
            .arg(info.chain.to_string())
            .arg(&deployed_address)
            .arg(contract_path)
            .arg("--constructor-args")
            .arg(encode_constructor_args_hex(
                42,
                "TestString",
                "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045",
            ))
            .arg("--zksync")
            .arg("--verifier-url")
            .arg(ZKSYNC_VERIFIER_URL)
            .arg("--verifier")
            .arg("zksync")
            .execute();

        let verify_stdout = String::from_utf8_lossy(&verify_result.stdout);
        let verify_stderr = String::from_utf8_lossy(&verify_result.stderr);

        assert!(
            verify_stdout.contains("Start verifying contract")
                || verify_stderr.contains("Start verifying contract")
                || verify_stderr.contains("no deployed contract")
        );
    }
}

forgetest!(test_zk_deploy_with_constructor_args_sepolia, |prj, cmd| {
    deploy_with_constructor_args(zk_env_externalities(), prj, cmd);
});

forgetest!(test_zk_create_then_verify_with_constructor_args_sepolia, |prj, cmd| {
    create_then_verify_with_constructor_args(zk_env_externalities(), prj, cmd);
});
