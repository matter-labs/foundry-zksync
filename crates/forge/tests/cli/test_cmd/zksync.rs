//! Contains various tests for `forge test --zksync`.

use foundry_config::{Config, FsPermissions, fs_permissions::PathPermission};
use foundry_test_utils::{
    TestCommand, TestProject, ZkSyncNode,
    rpc::rpc_endpoints_zk,
    util::{self, OutputExt},
};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

forgetest!(test_zk_core, |_prj, cmd| {
    let testdata =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata_zk").canonicalize().unwrap();
    cmd.current_dir(&testdata);

    let mut dotenv = std::fs::File::create(testdata.join(".env")).unwrap();
    writeln!(dotenv, "ZK_DEBUG_HISTORICAL_BLOCK_HASHES=5").unwrap();
    for (name, endpoint) in rpc_endpoints_zk().iter() {
        if let Some(url) = endpoint.endpoint.as_url() {
            let key = format!("RPC_{}", name.to_uppercase());
            cmd.env(&key, url);
            writeln!(dotenv, "{key}={url}").unwrap();
        }
    }
    drop(dotenv);

    let args = vec![
        "test",
        "--zksync",
        "--nmc",
        "(ZkSetupForkFailureTest|EvmInterpreterTest|Issue|ZkTraceTest)",
    ];

    let orig_assert = cmd.args(args).assert();
    if orig_assert.get_output().status.success() {
        return;
    }

    // Retry failed tests.
    cmd.args(["--rerun"]);
    let n = 3;
    for i in 1..=n {
        test_debug!("retrying failed tests... ({i}/{n})");
        let assert = cmd.assert();
        if assert.get_output().status.success() {
            return;
        }
    }

    orig_assert.success();
});

forgetest!(test_zk_traces, |_prj, cmd| {
    let testdata =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata_zk").canonicalize().unwrap();
    cmd.current_dir(&testdata);

    let mut dotenv = std::fs::File::create(testdata.join(".env")).unwrap();
    writeln!(dotenv, "ZK_DEBUG_HISTORICAL_BLOCK_HASHES=5").unwrap();
    for (name, endpoint) in rpc_endpoints_zk().iter() {
        if let Some(url) = endpoint.endpoint.as_url() {
            let key = format!("RPC_{}", name.to_uppercase());
            cmd.env(&key, url);
            writeln!(dotenv, "{key}={url}").unwrap();
        }
    }
    drop(dotenv);

    let args = vec!["test", "--zksync", "--mc", "ZkTraceTest", "-vvvvv"];

    cmd.args(args).assert_success().stdout_eq(str![[r#"
...
Ran 2 tests for Trace.t.sol:ZkTraceTest
[PASS] testZkTraceOutputDuringCall() ([GAS])
Logs:
  10

Traces:
  [..] ZkTraceTest::testZkTraceOutputDuringCall()
    ├─ [..] → new Adder@0xB5c1DF089600415B21FB76bf89900Adb575947c8
    │   └─ ← [Return] 2848 bytes of code
    ├─ [..] Adder::add()
    │   ├─ [..] → new Number@0xd6A7A38ee698eFae2F48F3a62dC7a71C3C0930A1
    │   │   └─ ← [Return] 2208 bytes of code
    │   ├─ [..] Number::five()
    │   │   ├─ [..] → new InnerNumber@0x89c74b24FB24DDa42a8465EE0F9edE2c1308DeEb
    │   │   │   └─ ← [Return] 800 bytes of code
    │   │   ├─ [..] InnerNumber::innerFive()
    │   │   │   └─ ← [Return] 5
    │   │   └─ ← [Return] 5
    │   ├─ [..] Number::five()
    │   │   ├─ [..] → new InnerNumber@0x9359008843d2c083a14E9C17Cde01893938047FA
    │   │   │   └─ ← [Return] 800 bytes of code
    │   │   ├─ [..] InnerNumber::innerFive()
    │   │   │   └─ ← [Return] 5
    │   │   └─ ← [Return] 5
    │   └─ ← [Return] 10
    ├─ [..] console::log(10) [staticcall]
    │   └─ ← [Stop]
    └─ ← [Stop]

[PASS] testZkTraceOutputDuringCreate() ([GAS])
Logs:
  10

Traces:
  [..] ZkTraceTest::testZkTraceOutputDuringCreate()
    ├─ [..] → new ConstructorAdder@0xB5c1DF089600415B21FB76bf89900Adb575947c8
    │   ├─ [..] → new Number@0xd6A7A38ee698eFae2F48F3a62dC7a71C3C0930A1
    │   │   └─ ← [Return] 2208 bytes of code
    │   ├─ [..] Number::five()
    │   │   ├─ [..] → new InnerNumber@0x89c74b24FB24DDa42a8465EE0F9edE2c1308DeEb
    │   │   │   └─ ← [Return] 800 bytes of code
    │   │   ├─ [..] InnerNumber::innerFive()
    │   │   │   └─ ← [Return] 5
    │   │   └─ ← [Return] 5
    │   ├─ [..] Number::five()
    │   │   ├─ [..] → new InnerNumber@0x9359008843d2c083a14E9C17Cde01893938047FA
    │   │   │   └─ ← [Return] 800 bytes of code
    │   │   ├─ [..] InnerNumber::innerFive()
    │   │   │   └─ ← [Return] 5
    │   │   └─ ← [Return] 5
    │   ├─ [..] console::log(10)
    │   │   └─ ← [Return]
    │   └─ ← [Return] 3040 bytes of code
    └─ ← [Stop]

Suite result: ok. 2 passed; 0 failed; 0 skipped; [ELAPSED]

Ran 1 test suite [ELAPSED]: 2 tests passed, 0 failed, 0 skipped (2 total tests)

"#]]);
});

forgetest!(test_zk_repros, |_prj, cmd| {
    let testdata =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata_zk").canonicalize().unwrap();
    cmd.current_dir(&testdata);

    let mut dotenv = std::fs::File::create(testdata.join(".env")).unwrap();
    writeln!(dotenv, "ZK_DEBUG_HISTORICAL_BLOCK_HASHES=5").unwrap();
    for (name, endpoint) in rpc_endpoints_zk().iter() {
        if let Some(url) = endpoint.endpoint.as_url() {
            let key = format!("RPC_{}", name.to_uppercase());
            cmd.env(&key, url);
            writeln!(dotenv, "{key}={url}").unwrap();
        }
    }
    drop(dotenv);

    let args = vec!["test", "--zksync", "--mc", "Issue"];

    let orig_assert = cmd.args(args).assert();
    if orig_assert.get_output().status.success() {
        return;
    }

    // Retry failed tests.
    cmd.args(["--rerun"]);
    let n = 3;
    for i in 1..=n {
        test_debug!("retrying failed tests... ({i}/{n})");
        let assert = cmd.assert();
        if assert.get_output().status.success() {
            return;
        }
    }

    orig_assert.success();
});

forgetest!(test_zk_failures, |_prj, cmd| {
    let testdata =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata_zk").canonicalize().unwrap();
    cmd.current_dir(&testdata);

    let mut dotenv = std::fs::File::create(testdata.join(".env")).unwrap();
    writeln!(dotenv, "ZK_DEBUG_HISTORICAL_BLOCK_HASHES=5").unwrap();
    for (name, endpoint) in rpc_endpoints_zk().iter() {
        if let Some(url) = endpoint.endpoint.as_url() {
            let key = format!("RPC_{}", name.to_uppercase());
            cmd.env(&key, url);
            writeln!(dotenv, "{key}={url}").unwrap();
        }
    }
    drop(dotenv);

    let args = vec!["test", "--zksync", "--mc", "ZkSetupForkFailureTest"];

    let orig_assert = cmd.args(args).assert();
    if !orig_assert.get_output().status.success() {
        return;
    }

    // Retry falsely succeeding tests.
    cmd.args(["--rerun"]);
    let n = 3;
    for i in 1..=n {
        test_debug!("retrying failed tests... ({i}/{n})");
        let assert = cmd.assert();
        if !assert.get_output().status.success() {
            return;
        }
    }

    orig_assert.failure();
});

forgetest!(test_zk_evm_interpreter, |_prj, cmd| {
    let testdata =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata_zk").canonicalize().unwrap();
    cmd.current_dir(&testdata);

    let mut dotenv = std::fs::File::create(testdata.join(".env")).unwrap();
    writeln!(dotenv, "ZK_DEBUG_HISTORICAL_BLOCK_HASHES=5").unwrap();
    for (name, endpoint) in rpc_endpoints_zk().iter() {
        if let Some(url) = endpoint.endpoint.as_url() {
            let key = format!("RPC_{}", name.to_uppercase());
            cmd.env(&key, url);
            writeln!(dotenv, "{key}={url}").unwrap();
        }
    }
    drop(dotenv);

    let args = vec![
        "test",
        "--zksync",
        "--zk-evm-interpreter=true",
        "--mc",
        "EvmInterpreterTest",
        "-vvvvv",
    ];

    let orig_assert = cmd.args(args).assert();
    if orig_assert.get_output().status.success() {
        return;
    }

    // Retry failed tests.
    cmd.args(["--rerun"]);
    let n = 3;
    for i in 1..=n {
        test_debug!("retrying failed tests... ({i}/{n})");
        let assert = cmd.assert();
        if assert.get_output().status.success() {
            return;
        }
    }

    orig_assert.success();
});

pub async fn run_zk_script_test(
    root: impl AsRef<std::path::Path>,
    cmd: &mut TestCommand,
    script_path: &str,
    contract_name: &str,
    dependencies: Option<&str>,
    expected_broadcastable_txs: usize,
    extra_args: Option<&[&str]>,
) {
    let node = ZkSyncNode::start().await;
    let url = node.url();

    if let Some(deps) = dependencies {
        let mut install_args = vec!["install"];
        install_args.extend(deps.split_whitespace());
        cmd.args(&install_args).assert_success();
    }

    cmd.forge_fuse();

    let script_path_contract = format!("{script_path}:{contract_name}");
    let private_key =
        ZkSyncNode::rich_wallets().next().map(|(_, pk, _)| pk).expect("No rich wallets available");

    let mut script_args = vec![
        "--zk-startup",
        &script_path_contract,
        "--private-key",
        &private_key,
        "--chain",
        "260",
        "--gas-estimate-multiplier",
        "310",
        "--rpc-url",
        url.as_str(),
        "--slow",
        "--evm-version",
        "shanghai",
    ];

    if let Some(args) = extra_args {
        script_args.extend_from_slice(args);
    }

    cmd.arg("script").args(&script_args);

    cmd.assert_success()
        .get_output()
        .stdout_lossy()
        .contains("ONCHAIN EXECUTION COMPLETE & SUCCESSFUL");

    let run_latest = foundry_common::fs::json_files(root.as_ref().join("broadcast").as_path())
        .find(|file| file.ends_with("run-latest.json"))
        .expect("No broadcast artifacts");

    let content = foundry_common::fs::read_to_string(run_latest).unwrap();

    let json: serde_json::Value = serde_json::from_str(&content).unwrap();

    assert_eq!(
        json["transactions"].as_array().expect("broadcastable txs").len(),
        expected_broadcastable_txs
    );
    cmd.forge_fuse();
}

pub fn deploy_zk_contract(
    cmd: &mut TestCommand,
    url: &str,
    private_key: &str,
    contract_path: &str,
    extra_args: Option<&[&str]>,
) -> Result<String, String> {
    cmd.forge_fuse().args([
        "create",
        "--zk-startup",
        contract_path,
        "--rpc-url",
        url,
        "--private-key",
        private_key,
        "--broadcast",
    ]);

    if let Some(args) = extra_args {
        cmd.args(args);
    }

    let output = cmd.assert_success();
    let output = output.get_output();
    let stdout = output.stdout_lossy();
    let stderr = foundry_test_utils::util::lossy_string(output.stderr.as_slice());

    if stdout.contains("Deployed to:") {
        let regex = regex::Regex::new(r"Deployed to:\s*(\S+)").unwrap();
        regex
            .captures(&stdout)
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str().to_string())
            .ok_or_else(|| "Failed to extract deployed address".to_string())
    } else {
        Err(format!("Deployment failed. Stdout: {stdout}\nStderr: {stderr}"))
    }
}

mod cheats {
    use super::*;

    fn setup_deploy_prj(prj: &mut TestProject) {
        util::initialize(prj.root());
        let permissions = FsPermissions::new(vec![
            PathPermission::read(Path::new("zkout/Counter.sol/Counter.json")),
            PathPermission::read(Path::new("zkout/Factory.sol/Factory.json")),
        ]);
        let config = Config { fs_permissions: permissions, ..Default::default() };
        prj.write_config(config);
        prj.add_script(
            "DeployCounterWithBytecodeHash.s.sol",
            include_str!("../../fixtures/zk/DeployCounterWithBytecodeHash.s.sol"),
        );
        prj.add_source("Factory.sol", include_str!("../../fixtures/zk/Factory.sol"));
        prj.add_source("Counter", "contract Counter {}");
    }

    forgetest_async!(test_zk_use_factory_dep, |prj, cmd| {
        setup_deploy_prj(&mut prj);

        cmd.forge_fuse();
        // We added the optimizer flag which is now false by default so we need to set it to true
        run_zk_script_test(
        prj.root(),
        &mut cmd,
        "./script/DeployCounterWithBytecodeHash.s.sol",
        "DeployCounterWithBytecodeHash",
        Some("transmissions11/solmate@v7 OpenZeppelin/openzeppelin-contracts cyfrin/zksync-contracts"),
        2,
        Some(&["-vvvvv", "--via-ir", "--system-mode=true", "--broadcast", "--optimize=true"]),
    ).await;
    });

    forgetest_async!(script_zk_broadcast_raw_in_output_json, |prj, cmd| {
        util::initialize(prj.root());
        let node = ZkSyncNode::start().await;
        let (_, private_key) = ZkSyncNode::rich_wallets()
            .next()
            .map(|(addr, pk, _)| (addr, pk))
            .expect("No rich wallets available");

        prj.add_source(
            "Counter.sol",
            r#"
pragma solidity ^0.8.0;

contract Counter {
    uint256 public count;
    function increment() external {
        count++;
    }
}
    "#,
        );

        prj.add_script(
    "SimpleScript.s.sol",
    r#"
import "forge-std/Script.sol";
import {Counter} from "../src/Counter.sol";
contract SimpleScript is Script {
    function run() external {
        // zk raw transaction
        vm.startBroadcast();
        
        // cast mktx --rpc-url &url --private-key &private_key --zksync --create  "0x000000800300003900 .... " // <-- The bytecode comes from the the Counter contract above
        vm.broadcastRawTransaction(
        hex"71f903ae80808402b275d0834020b694000000000000000000000000000000000000800680b8849c4d535b000000000000000000000000000000000000000000000000000000000000000001000015eef9c25753fdda281d958bddd70867399645894ec102744c554999fd0000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000000080a005820228fcbfc5a16562ad38e4f7976de211f827c9f8b554f0f4587002d3cd3ba05de935fcae8d2e93fe31029a3c1cb9f6d3c6255e979917e030f5731f6383495782010494bc989fde9e54cad2ab4392af6df60f04873a033a830f4240f902a3b902a00000008003000039000000400030043f0000000100200190000000130000c13d0000000d00100198000000270000613d000000000101043b000000e0011002700000000e0010009c0000001b0000613d0000000f0010009c000000270000c13d0000000001000416000000000001004b000000270000c13d000000000100041a000000800010043f00000012010000410000002d0001042e0000000001000416000000000001004b000000270000c13d0000002001000039000001000010044300000120000004430000000c010000410000002d0001042e0000000001000416000000000001004b000000270000c13d000000000100041a000000010110003a000000290000c13d0000001001000041000000000010043f0000001101000039000000040010043f00000011010000410000002e0001043000000000010000190000002e00010430000000000010041b00000000010000190000002d0001042e0000002c000004320000002d0001042e0000002e000104300000000000000000000000020000000000000000000000000000004000000100000000000000000000000000000000000000000000000000fffffffc00000000000000000000000000000000000000000000000000000000000000000000000000000000d09de08a0000000000000000000000000000000000000000000000000000000006661abd4e487b7100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002400000000000000000000000000000000000000000000000000000000000000200000008000000000000000000000000000000000000000000000000000000000000000000000000000000000b34d28be4c2607700146bb47d0b91f49fda73afa4b2b4cccde86e0ea842d71ed8080"
        );

        Counter counter = Counter(0x9c1a3d7C98dBF89c7f5d167F2219C29c2fe775A7);
        uint256 prev = counter.count();
        require(prev == 0);

        // `cast mktx "0x9c1a3d7C98dBF89c7f5d167F2219C29c2fe775A7" "increment()" --rpc-url http://127.0.0.1:49204 --private-key "0x3d3cbc973389cb26f657686445bcc75662b415b656078503592ac8c1abb8810e" --zksync --nonce 1`
        vm.broadcastRawTransaction(
            hex"71f88501808402b275d08304d0e4949c1a3d7c98dbf89c7f5d167f2219c29c2fe775a78084d09de08a80a0bda10084650ce446c1c22d72a98ed4489040c7702a6ea97d5e9f5ec3bb76a55da04c97363c5491dedd78dd955f2903338f75325836218f59cf62e9acdf454bbf3082010494bc989fde9e54cad2ab4392af6df60f04873a033a80c08080"            
        );

        require(counter.count() == prev + 1);

        vm.stopBroadcast();
    }
}
"#,
    );

        cmd.forge_fuse();

        cmd.args([
            "script",
            "-vvvvv",
            "--zksync",
            "--private-key",
            private_key,
            "--rpc-url",
            node.url().as_str(),
            "--broadcast",
            "--slow",
            "--non-interactive",
            "SimpleScript",
        ])
        .assert_success();

        let run_latest = foundry_common::fs::json_files(prj.root().join("broadcast").as_path())
            .find(|file| file.ends_with("run-latest.json"))
            .expect("No broadcast artifacts");

        let content = foundry_common::fs::read_to_string(run_latest).unwrap();
        let json: serde_json::Value = serde_json::from_str(&content).unwrap();
        let txns = json["transactions"].as_array().expect("broadcastable txs");

        let deployment = &txns[0];
        deployment["contractAddress"]
            .as_str()
            .expect("contract address key was not a string")
            .contains("0x0000000000000000000000000000000000008006");
        deployment["transaction"]["from"]
            .as_str()
            .expect("from key was not a string")
            .contains("0xbc989fde9e54cad2ab4392af6df60f04873a033a");

        let zksync =
            deployment["transaction"]["zksync"].as_object().expect("zksync key was not an object");
        let factory_deps = zksync["factoryDeps"].as_array().expect("factoryDeps was not an array");
        assert!(!factory_deps.is_empty());

        let broadcasted = &txns[1];

        // check that the broadcasted tx have the correct function and contract address
        assert_eq!(txns.len(), 2);
        broadcasted["function"]
            .as_str()
            .expect("function name key was not a string")
            .contains("increment");
        broadcasted["contractAddress"]
            .as_str()
            .expect("contract address key was not a string")
            .contains("0x9c1a3d7c98dbf89c7f5d167f2219c29c2fe775a7");

        broadcasted["transaction"]["from"]
            .as_str()
            .expect("from was key not a string")
            .contains("0xbc989fde9e54cad2ab4392af6df60f04873a033a");
        broadcasted["transaction"]["to"]
            .as_str()
            .expect("to was not key a string")
            .contains("0x9c1a3d7c98dbf89c7f5d167f2219c29c2fe775a7");
        broadcasted["transaction"]["value"]
            .as_str()
            .expect("value was not a string")
            .contains("0x0");
    });
}

mod constructor {
    use super::*;

    forgetest_async!(test_zk_constructor_works_in_script, |prj, cmd| {
        setup_deploy_prj(&mut prj);
        run_zk_script_test(
            prj.root(),
            &mut cmd,
            "./script/Constructor.s.sol",
            "ConstructorScript",
            None,
            3,
            Some(&["-vvvvv", "--broadcast"]),
        )
        .await;
    });

    fn setup_deploy_prj(prj: &mut TestProject) {
        util::initialize(prj.root());
        prj.add_script("Constructor.s.sol", include_str!("../../fixtures/zk/Constructor.s.sol"));
        prj.add_source("Bank.sol", include_str!("../../../../../testdata_zk/Bank.sol"));
    }
}

mod contracts {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_zk_contract_create2() {
        let (prj, mut cmd) = util::setup_forge(
            "test_zk_contract_create2_with_deps",
            foundry_test_utils::foundry_compilers::PathStyle::Dapptools,
        );
        util::initialize(prj.root());

        cmd.forge_fuse().args(["install", "cyfrin/zksync-contracts", "--shallow"]).assert_success();
        cmd.forge_fuse();

        let mut config = cmd.config();
        config.fs_permissions.add(PathPermission::read("./zkout"));
        prj.write_config(config);

        prj.add_source("Greeter.sol", include_str!("../../../../../testdata_zk/Greeter.sol"));

        prj.add_source(
            "CustomNumber.sol",
            include_str!("../../../../../testdata_zk/CustomNumber.sol"),
        );

        prj.add_source(
            "Create2Utils.sol",
            include_str!("../../../../../testdata_zk/Create2Utils.sol"),
        );

        prj.add_test("Create2.t.sol", include_str!("../../fixtures/zk/Create2.t.sol"));

        cmd.args([
            "test",
            "--zk-startup",
            "--evm-version",
            "shanghai",
            "--mc",
            "Create2Test",
            "--optimize",
            "true",
        ]);
        cmd.assert_success().get_output().stdout_lossy().contains("Suite result: ok");
    }
}

mod create {
    use super::*;

    forgetest_async!(forge_zk_can_deploy_erc20, |prj, cmd| {
        util::initialize(prj.root());
        prj.add_source("ERC20.sol", include_str!("../../../../../testdata_zk/ERC20.sol"));

        let node = ZkSyncNode::start().await;
        let url = node.url();

        let private_key = ZkSyncNode::rich_wallets()
            .next()
            .map(|(_, pk, _)| pk)
            .expect("No rich wallets available");

        let erc20_address = deploy_zk_contract(
            &mut cmd,
            url.as_str(),
            private_key,
            "./src/ERC20.sol:MyToken",
            None,
        )
        .expect("Failed to deploy ERC20 contract");

        assert!(!erc20_address.is_empty(), "Deployed address should not be empty");
    });

    forgetest_async!(forge_zk_can_deploy_contracts_and_cast_a_transaction, |prj, cmd| {
        util::initialize(prj.root());
        prj.add_source(
            "TokenReceiver.sol",
            include_str!("../../../../../testdata_zk/TokenReceiver.sol"),
        );
        prj.add_source("ERC20.sol", include_str!("../../../../../testdata_zk/ERC20.sol"));

        let node = ZkSyncNode::start().await;
        let url = node.url();

        let private_key = ZkSyncNode::rich_wallets()
            .next()
            .map(|(_, pk, _)| pk)
            .expect("No rich wallets available");

        let token_receiver_address = deploy_zk_contract(
            &mut cmd,
            url.as_str(),
            private_key,
            "./src/TokenReceiver.sol:TokenReceiver",
            None,
        )
        .expect("Failed to deploy TokenReceiver contract");
        let erc_20_address = deploy_zk_contract(
            &mut cmd,
            url.as_str(),
            private_key,
            "./src/ERC20.sol:MyToken",
            None,
        )
        .expect("Failed to deploy ERC20 contract");

        cmd.cast_fuse().args([
            "send",
            "--rpc-url",
            url.as_str(),
            "--private-key",
            private_key,
            &erc_20_address,
            "transfer(address,uint256)",
            &token_receiver_address,
            "1",
        ]);

        let stdout = cmd.assert_success().get_output().stdout_lossy();

        assert!(stdout.contains("transactionHash"), "Transaction hash not found in output");
        assert!(stdout.contains("success"), "Transaction was not successful");
    });

    forgetest_async!(forge_zk_can_deploy_contracts_with_gas_per_pubdata_and_succeed, |prj, cmd| {
        util::initialize(prj.root());
        prj.add_source("ERC20.sol", include_str!("../../../../../testdata_zk/ERC20.sol"));

        let node = ZkSyncNode::start().await;
        let url = node.url();

        let private_key = ZkSyncNode::rich_wallets()
            .next()
            .map(|(_, pk, _)| pk)
            .expect("No rich wallets available");

        deploy_zk_contract(
            &mut cmd,
            url.as_str(),
            private_key,
            "./src/ERC20.sol:MyToken",
            Some(&["--zk-gas-per-pubdata", "3000"]),
        )
        .expect("Failed to deploy ERC20 contract");
    });

    forgetest_async!(
        forge_zk_can_deploy_contracts_with_invalid_gas_per_pubdata_and_fail,
        |prj, cmd| {
            util::initialize(prj.root());
            prj.add_source("ERC20.sol", include_str!("../../../../../testdata_zk/ERC20.sol"));

            let node = ZkSyncNode::start().await;
            let url = node.url();

            let private_key = ZkSyncNode::rich_wallets()
                .next()
                .map(|(_, pk, _)| pk)
                .expect("No rich wallets available");
            cmd.forge_fuse().args([
                "create",
                "--zk-startup",
                "./src/ERC20.sol:MyToken",
                "--rpc-url",
                url.as_str(),
                "--private-key",
                private_key,
                "--broadcast",
                "--timeout",
                "1",
                "--zk-gas-per-pubdata",
                "1",
            ]);

            cmd.assert_failure();
        }
    );

    forgetest_async!(forge_zk_create_dry_run_without_broadcast, |prj, cmd| {
        util::initialize(prj.root());
        prj.add_source("ERC20.sol", include_str!("../../../../../testdata_zk/ERC20.sol"));

        let node = ZkSyncNode::start().await;
        let url = node.url();

        let private_key = ZkSyncNode::rich_wallets()
            .next()
            .map(|(_, pk, _)| pk)
            .expect("No rich wallets available");

        // Test dry-run behavior WITHOUT --broadcast flag
        cmd.forge_fuse().args([
            "create",
            "--zk-startup",
            "./src/ERC20.sol:MyToken",
            "--rpc-url",
            url.as_str(),
            "--private-key",
            private_key,
        ]);

        let output = cmd.assert_success().get_output().stdout_lossy();

        // Should show transaction details (this proves dry-run is working)
        assert!(output.contains("Contract: MyToken"), "Expected contract name in output");
        assert!(output.contains("Transaction:"), "Expected transaction details in output");
        assert!(output.contains("ABI:"), "Expected ABI output in output");

        // Should NOT show deployment success messages (this is the key test)
        assert!(!output.contains("Deployed to:"), "Should not show deployment address in dry-run");
        assert!(
            !output.contains("Transaction hash:"),
            "Should not show transaction hash in dry-run"
        );
    });

    forgetest_async!(forge_zk_create_with_broadcast_flag, |prj, cmd| {
        util::initialize(prj.root());
        prj.add_source("ERC20.sol", include_str!("../../../../../testdata_zk/ERC20.sol"));

        let node = ZkSyncNode::start().await;
        let url = node.url();

        let private_key = ZkSyncNode::rich_wallets()
            .next()
            .map(|(_, pk, _)| pk)
            .expect("No rich wallets available");

        // Test actual deployment WITH --broadcast flag (helper function already includes
        // --broadcast)
        let deployed_address = deploy_zk_contract(
            &mut cmd,
            url.as_str(),
            private_key,
            "./src/ERC20.sol:MyToken",
            None,
        )
        .expect("Failed to deploy ERC20 contract");

        // Should successfully deploy and return an address
        assert!(!deployed_address.is_empty(), "Deployed address should not be empty");
        assert!(deployed_address.starts_with("0x"), "Address should be a valid hex address");
    });
}

mod create2 {
    use super::*;

    forgetest_async!(can_deploy_via_create2, |prj, cmd| {
        setup_create2_prj(&mut prj);
        let mut config = cmd.config();
        config.fs_permissions.add(PathPermission::read("./zkout"));
        prj.write_config(config);
        run_zk_script_test(
            prj.root(),
            &mut cmd,
            "./script/Create2.s.sol",
            "Create2Script",
            None,
            2,
            Some(&["-vvvvv", "--broadcast"]),
        )
        .await;
    });

    fn setup_create2_prj(prj: &mut TestProject) {
        util::initialize(prj.root());
        prj.add_script("Create2.s.sol", include_str!("../../fixtures/zk/Create2.s.sol"));
        prj.add_source("Greeter.sol", include_str!("../../../../../testdata_zk/Greeter.sol"));
        prj.add_source(
            "Create2Utils.sol",
            include_str!("../../../../../testdata_zk/Create2Utils.sol"),
        );
    }
}

mod deploy {
    use super::*;

    forgetest_async!(multiple_deployments_of_the_same_contract, |prj, cmd| {
        setup_deploy_prj(&mut prj);
        run_zk_script_test(
            prj.root(),
            &mut cmd,
            "./script/Deploy.s.sol",
            "DeployScript",
            None,
            3,
            Some(&["-vvvvv", "--broadcast"]),
        )
        .await;
        run_zk_script_test(
            prj.root(),
            &mut cmd,
            "./script/Deploy.s.sol",
            "DeployScript",
            None,
            3,
            Some(&["-vvvvv", "--broadcast"]),
        )
        .await;
    });

    fn setup_deploy_prj(prj: &mut TestProject) {
        util::initialize(prj.root());
        prj.add_script("Deploy.s.sol", include_str!("../../fixtures/zk/Deploy.s.sol"));
        prj.add_source("Greeter.sol", include_str!("../../../../../testdata_zk/Greeter.sol"));
    }
}

mod factory_deps {
    use foundry_zksync_core::MAX_L2_GAS_LIMIT;

    use super::*;

    forgetest_async!(script_zk_can_deploy_large_factory_deps, |prj, cmd| {
        util::initialize(prj.root());

        prj.add_source(
            "LargeContracts.sol",
            include_str!("../../../../../testdata_zk/LargeContracts.sol"),
        );
        prj.add_script(
            "LargeContracts.s.sol",
            r#"
import "forge-std/Script.sol";
import "../src/LargeContracts.sol";

contract ZkLargeFactoryDependenciesScript is Script {
    function run() external {
        vm.broadcast();
        new LargeContract();
    }
}
"#,
        );

        let node = ZkSyncNode::start().await;

        // foundry default gas-limit is not enough to pay for factory deps
        // with Anvil-zksync's environment
        let gas_limit = MAX_L2_GAS_LIMIT;

        cmd.arg("script").args([
            "--zk-startup",
            "./script/LargeContracts.s.sol",
            "--broadcast",
            "--private-key",
            "0x3d3cbc973389cb26f657686445bcc75662b415b656078503592ac8c1abb8810e",
            "--chain",
            "260",
            "--gas-estimate-multiplier",
            "310",
            "--rpc-url",
            node.url().as_str(),
            "--slow",
            "--gas-limit",
            &gas_limit.to_string(),
        ]);
        cmd.assert_success()
            .get_output()
            .stdout_lossy()
            .contains("ONCHAIN EXECUTION COMPLETE & SUCCESSFUL");

        let run_latest = foundry_common::fs::json_files(prj.root().join("broadcast").as_path())
            .find(|file| file.ends_with("run-latest.json"))
            .expect("No broadcast artifacts");

        let content = foundry_common::fs::read_to_string(run_latest).unwrap();

        let json: serde_json::Value = serde_json::from_str(&content).unwrap();
        let txns = json["transactions"].as_array().expect("broadcastable txs");
        assert_eq!(txns.len(), 3);

        // check that the txs have strictly monotonically increasing nonces
        assert!(txns.iter().filter_map(|tx| tx["nonce"].as_u64()).is_sorted_by(|a, b| a + 1 == *b));
    });
}

mod factory {
    use super::*;

    forgetest_async!(script_zk_can_deploy_in_method, |prj, cmd| {
        setup_factory_prj(&mut prj);
        run_zk_script_test(
            prj.root(),
            &mut cmd,
            "./script/Factory.s.sol",
            "ZkClassicFactoryScript",
            None,
            2,
            Some(&["--broadcast"]),
        )
        .await;
        run_zk_script_test(
            prj.root(),
            &mut cmd,
            "./script/Factory.s.sol",
            "ZkNestedFactoryScript",
            None,
            2,
            Some(&["--broadcast"]),
        )
        .await;
    });

    forgetest_async!(script_zk_can_deploy_in_constructor, |prj, cmd| {
        setup_factory_prj(&mut prj);
        run_zk_script_test(
            prj.root(),
            &mut cmd,
            "./script/Factory.s.sol",
            "ZkConstructorFactoryScript",
            None,
            1,
            Some(&["--broadcast"]),
        )
        .await;
        run_zk_script_test(
            prj.root(),
            &mut cmd,
            "./script/Factory.s.sol",
            "ZkNestedConstructorFactoryScript",
            None,
            1,
            Some(&["--broadcast"]),
        )
        .await;
    });

    forgetest_async!(script_zk_can_use_predeployed_factory, |prj, cmd| {
        setup_factory_prj(&mut prj);
        run_zk_script_test(
            prj.root(),
            &mut cmd,
            "./script/Factory.s.sol",
            "ZkUserFactoryScript",
            None,
            3,
            Some(&["--broadcast"]),
        )
        .await;
        run_zk_script_test(
            prj.root(),
            &mut cmd,
            "./script/Factory.s.sol",
            "ZkUserConstructorFactoryScript",
            None,
            2,
            Some(&["--broadcast"]),
        )
        .await;
    });

    fn setup_factory_prj(prj: &mut TestProject) {
        util::initialize(prj.root());
        prj.add_source("Factory.sol", include_str!("../../../../../testdata_zk/Factory.sol"));
        prj.add_script("Factory.s.sol", include_str!("../../fixtures/zk/Factory.s.sol"));
    }
}

mod fork {
    use alloy_provider::Provider;
    use foundry_common::provider::try_get_zksync_http_provider;
    use foundry_zksync_core::state::{get_nonce_storage, new_full_nonce};

    use super::*;

    forgetest_async!(test_zk_consistent_nonce_migration_after_fork, |prj, cmd| {
        let test_address = alloy_primitives::address!("076d6da60aAAC6c97A8a0fE8057f9564203Ee545");
        let transaction_nonce = 2;
        let deployment_nonce = 1;

        let node = ZkSyncNode::start().await;
        // set nonce
        let (nonce_key_addr, nonce_key_slot) = get_nonce_storage(test_address);
        let full_nonce = new_full_nonce(transaction_nonce, deployment_nonce);
        let result = try_get_zksync_http_provider(node.url())
            .unwrap()
            .raw_request::<_, bool>(
                "anvil_setStorageAt".into(),
                (nonce_key_addr, nonce_key_slot, full_nonce),
            )
            .await
            .unwrap();
        assert!(result, "failed setting nonce on anvil-zksync");

        // prepare script
        util::initialize(prj.root());
        prj.add_script(
        "ZkForkNonceTest.s.sol",
        format!(r#"
import "forge-std/Script.sol";
import "forge-std/Test.sol";

interface VmExt {{
    function zkGetTransactionNonce(
        address account
    ) external view returns (uint64 nonce);
    function zkGetDeploymentNonce(
        address account
    ) external view returns (uint64 nonce);
}}

contract ZkForkNonceTest is Script {{
    VmExt internal constant vmExt = VmExt(VM_ADDRESS);

    address constant TEST_ADDRESS = {test_address};
    uint128 constant TEST_ADDRESS_TRANSACTION_NONCE = {transaction_nonce};
    uint128 constant TEST_ADDRESS_DEPLOYMENT_NONCE = {deployment_nonce};

    function run() external {{
        require(TEST_ADDRESS_TRANSACTION_NONCE == vmExt.zkGetTransactionNonce(TEST_ADDRESS), "failed matching transaction nonce");
        require(TEST_ADDRESS_DEPLOYMENT_NONCE == vmExt.zkGetDeploymentNonce(TEST_ADDRESS), "failed matching deployment nonce");
    }}
}}
"#).as_str(),
    );

        cmd.arg("script")
            .args([
                "--zk-startup=true",
                "--no-storage-caching", // prevents rpc caching
                "--rpc-url",
                node.url().as_str(),
                // set address as sender to be migrated on startup, so storage is read immediately
                "--sender",
                test_address.to_string().as_str(),
            ])
            .arg("ZkForkNonceTest"); // Contract name as the PATH argument

        cmd.assert_success();
    });
}

mod gas {
    use super::*;

    forgetest_async!(zk_script_execution_with_gas_price_specified_by_user, |prj, cmd| {
        // Setup
        setup_gas_prj(&mut prj);
        let node = ZkSyncNode::start().await;
        let url = node.url();
        cmd.forge_fuse();
        let private_key = get_rich_wallet_key();

        // Create script args with gas price parameters
        let script_args =
            create_script_args(&private_key, url.as_str(), "--with-gas-price", "370000037");
        let mut script_args = script_args.into_iter().collect::<Vec<_>>();
        script_args.extend_from_slice(&["--priority-gas-price", "123123"]);

        // Execute script and verify success
        cmd.arg("script").args(&script_args);
        let stdout = cmd.assert_success().get_output().stdout_lossy();
        assert!(stdout.contains("ONCHAIN EXECUTION COMPLETE & SUCCESSFUL"));

        // Verify transaction details from broadcast artifacts
        let run_latest = foundry_common::fs::json_files(prj.root().join("broadcast").as_path())
            .find(|file| file.ends_with("run-latest.json"))
            .expect("No broadcast artifacts");

        let json: serde_json::Value =
            serde_json::from_str(&foundry_common::fs::read_to_string(run_latest).unwrap()).unwrap();

        assert_eq!(json["transactions"].as_array().expect("broadcastable txs").len(), 1);

        // Verify gas prices in transaction
        let transaction_hash = json["receipts"][0]["transactionHash"].as_str().unwrap();
        let stdout = cmd
            .cast_fuse()
            .arg("tx")
            .arg(transaction_hash)
            .arg("--rpc-url")
            .arg(url.as_str())
            .assert_success()
            .get_output()
            .stdout_lossy();

        assert!(stdout.contains("maxFeePerGas         370000037"));
        assert!(stdout.contains("maxPriorityFeePerGas 123123"));
    });

    forgetest_async!(zk_script_execution_with_gas_multiplier, |prj, cmd| {
        // Setup
        setup_gas_prj(&mut prj);
        let node = ZkSyncNode::start().await;
        let url = node.url();
        cmd.forge_fuse();
        let private_key = get_rich_wallet_key();

        // Test with insufficient gas multiplier (should fail)
        let mut insufficient_multiplier_args =
            create_script_args(&private_key, &url, "--gas-estimate-multiplier", "1");

        // TODO(zk): `cast` currently hangs if transaction is dropped from mempool, thus we add
        // timeout for transactions. See https://github.com/alloy-rs/alloy/issues/2678
        insufficient_multiplier_args.push("--timeout");
        insufficient_multiplier_args.push("5");

        cmd.arg("script").args(&insufficient_multiplier_args);
        cmd.assert_failure();
        cmd.forge_fuse();

        // Test with sufficient gas multiplier (should succeed)
        let sufficient_multiplier_args =
            create_script_args(&private_key, &url, "--gas-estimate-multiplier", "100");
        cmd.arg("script").args(&sufficient_multiplier_args);
        let stdout = cmd.assert_success().get_output().stdout_lossy();
        assert!(stdout.contains("ONCHAIN EXECUTION COMPLETE & SUCCESSFUL"));
    });

    forgetest_async!(zk_script_execution_with_gas_per_pubdata, |prj, cmd| {
        // Setup
        setup_gas_prj(&mut prj);
        let node = ZkSyncNode::start().await;
        let url = node.url();
        cmd.forge_fuse();
        let private_key = get_rich_wallet_key();

        // Test with unacceptable gas per pubdata (should fail)
        let mut forge_bin = prj.forge_bin();
        // We had to change the approach of testing an invalid gas per pubdata value because there
        // were changes upstream for the timeout and retries mechanism Now we execute the
        // command directly and check the output with a manual timeout. The previous
        // approach was to use the `forge script` command with a timeout but now it's not
        // timeouting anymore for this error.
        let mut child = forge_bin
            .args([
                "script",
                "--zksync",
                "script/Gas.s.sol:GasScript",
                "--private-key",
                &private_key,
                "--chain",
                "260",
                "--rpc-url",
                &url,
                "--slow",
                "-vvvvv",
                "--broadcast",
                "--zk-gas-per-pubdata",
                "1",
            ])
            .current_dir(prj.root())
            .spawn()
            .expect("failed to spawn process");

        // Wait for 10 seconds then kill the process
        std::thread::sleep(std::time::Duration::from_secs(10));
        child.kill().expect("failed to kill process");
        let output = child.wait().expect("failed to wait for process");

        // Assert command was killed
        assert!(!output.success());

        // Test with sufficient gas per pubdata (should succeed)
        let sufficient_pubdata_args =
            create_script_args(&private_key, &url, "--zk-gas-per-pubdata", "3000");
        cmd.arg("script").args(&sufficient_pubdata_args);
        let stdout = cmd.assert_success().get_output().stdout_lossy();
        assert!(stdout.contains("ONCHAIN EXECUTION COMPLETE & SUCCESSFUL"));
    });

    fn get_rich_wallet_key() -> String {
        ZkSyncNode::rich_wallets()
            .next()
            .map(|(_, pk, _)| pk)
            .expect("No rich wallets available")
            .to_owned()
    }

    fn create_script_args<'a>(
        private_key: &'a str,
        url: &'a str,
        gas_param: &'a str,
        gas_value: &'a str,
    ) -> Vec<&'a str> {
        vec![
            "--zk-startup",
            "./script/Gas.s.sol",
            "--private-key",
            private_key,
            "--chain",
            "260",
            "--rpc-url",
            url,
            "--slow",
            "-vvvvv",
            "--broadcast",
            "--timeout",
            "3",
            gas_param,
            gas_value,
        ]
    }

    fn setup_gas_prj(prj: &mut TestProject) {
        util::initialize(prj.root());
        prj.add_script("Gas.s.sol", include_str!("../../fixtures/zk/Gas.s.sol"));
        prj.add_source("Greeter.sol", include_str!("../../../../../testdata_zk/Greeter.sol"));
    }
}

mod linking {
    use super::*;
    use semver::Version;

    const ZKSOLC_MIN_LINKING_VERSION: Version = Version::new(1, 5, 9);

    // TODO(zk): add equivalent test for `GetCodeUnlinked`
    // would probably need to split in separate file (and skip other file)
    // as tests look for _all_ lib deps and deploy them for every test

    forgetest_async!(
        #[should_panic = "no bytecode for contract; is it abstract or unlinked?"]
        script_zk_fails_indirect_reference_to_unlinked,
        |prj, cmd| {
            setup_libs_prj(&mut prj, &mut cmd, None);
            run_zk_script_test(
                prj.root(),
                &mut cmd,
                "./script/Libraries.s.sol",
                "GetCodeUnlinked",
                None,
                1,
                Some(&["-vvvvv"]),
            )
            .await;
        }
    );

    forgetest_async!(script_zk_deploy_time_linking, |prj, cmd| {
        setup_libs_prj(&mut prj, &mut cmd, None);
        run_zk_script_test(
            prj.root(),
            &mut cmd,
            "./script/Libraries.s.sol",
            "DeployTimeLinking",
            None,
            // lib `Foo` + `UsesFoo` deployment
            2,
            Some(&["-vvvvv", "--broadcast"]),
        )
        .await;
    });

    forgetest_async!(
        #[ignore]
        #[should_panic = "deploy-time linking not supported"]
        script_zk_deploy_time_linking_fails_older_version,
        |prj, cmd| {
            let mut version = ZKSOLC_MIN_LINKING_VERSION;
            version.patch -= 1;

            setup_libs_prj(&mut prj, &mut cmd, Some(version));
            run_zk_script_test(
                prj.root(),
                &mut cmd,
                "./script/Libraries.s.sol",
                "DeployTimeLinking",
                None,
                1,
                Some(&["-vvvvv"]),
            )
            .await;
        }
    );

    forgetest_async!(
        #[should_panic = "Dynamic linking not supported"]
        create_zk_using_unlinked_fails,
        |prj, cmd| {
            setup_libs_prj(&mut prj, &mut cmd, None);

            // we don't really connect to the rpc because
            // we expect to fail before that point
            let foo_address = deploy_zk_contract(
                &mut cmd,
                "127.0.0.1:1234",
                "0x0000000000000000000000000000000000000000000000000000000000000000",
                "./src/WithLibraries.sol:UsesFoo",
                None,
            )
            .expect("Failed to deploy UsesFoo contract");

            assert!(!foo_address.is_empty(), "Deployed address should not be empty");
        }
    );

    fn setup_libs_prj(prj: &mut TestProject, cmd: &mut TestCommand, zksolc: Option<Version>) {
        util::initialize(prj.root());

        let mut config = cmd.config();
        if let Some(zksolc) = zksolc {
            config.zksync.zksolc.replace(foundry_config::SolcReq::Version(zksolc));
        }
        prj.write_config(config);

        prj.add_script("Libraries.s.sol", include_str!("../../fixtures/zk/Libraries.s.sol"));
        prj.add_source(
            "WithLibraries.sol",
            include_str!("../../../../../testdata_zk/WithLibraries.sol"),
        );
    }
}

mod nft {
    use super::*;

    forgetest_async!(script_zk_can_deploy_nft, |prj, cmd| {
        setup_nft_prj(&mut prj);
        run_zk_script_test(
            prj.root(),
            &mut cmd,
            "./script/NFT.s.sol",
            "MyScript",
            Some("transmissions11/solmate@v7 OpenZeppelin/openzeppelin-contracts"),
            1,
            Some(&["-vvvvv", "--broadcast"]),
        )
        .await;
    });

    fn setup_nft_prj(prj: &mut TestProject) {
        util::initialize(prj.root());
        prj.add_script("NFT.s.sol", include_str!("../../fixtures/zk/NFT.s.sol"));
    }
}

mod nonce {
    use super::*;

    forgetest_async!(setup_block_on_script_test, |prj, cmd| {
        setup_deploy_prj(&mut prj);
        run_zk_script_test(
            prj.root(),
            &mut cmd,
            "./script/ScriptSetup.s.sol",
            "ScriptSetupNonce",
            None,
            3,
            Some(&["-vvvvv"]),
        )
        .await;
    });

    forgetest_async!(setup_broadcast_in_setup_test, |prj, cmd| {
        setup_deploy_prj(&mut prj);
        run_zk_script_test(
            prj.root(),
            &mut cmd,
            "./script/ScriptBroadcastInSetup.s.sol",
            "ScriptBroadcastInSetup",
            None,
            4,
            Some(&["-vvvvv", "--broadcast"]),
        )
        .await;
    });

    fn setup_deploy_prj(prj: &mut TestProject) {
        util::initialize(prj.root());
        prj.add_script("ScriptSetup.s.sol", include_str!("../../fixtures/zk/ScriptSetup.s.sol"));
        prj.add_script(
            "ScriptBroadcastInSetup.s.sol",
            include_str!("../../fixtures/zk/ScriptBroadcastInSetup.s.sol"),
        );
        prj.add_source("Greeter.sol", include_str!("../../../../../testdata_zk/Greeter.sol"));
    }
}

mod paymaster {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_zk_contract_paymaster() {
        let (prj, mut cmd) = util::setup_forge(
            "test_zk_contract_paymaster",
            foundry_test_utils::foundry_compilers::PathStyle::Dapptools,
        );
        util::initialize(prj.root());

        cmd.args([
            "install",
            "OpenZeppelin/openzeppelin-contracts",
            "cyfrin/zksync-contracts",
            "--shallow",
        ])
        .assert_success();
        cmd.forge_fuse();

        let config = cmd.config();
        prj.write_config(config);

        prj.add_source("MyPaymaster.sol", include_str!("../../fixtures/zk/MyPaymaster.sol"));
        prj.add_source("Paymaster.t.sol", include_str!("../../fixtures/zk/Paymaster.t.sol"));

        cmd.args([
            "test",
            "--zk-startup",
            "--via-ir",
            "--match-contract",
            "TestPaymasterFlow",
            "--optimize",
            "true",
        ]);
        assert!(cmd.assert_success().get_output().stdout_lossy().contains("Suite result: ok"));
    }

    // Tests the deployment of contracts using a paymaster for fee abstraction
    forgetest_async!(test_zk_deploy_with_paymaster, |prj, cmd| {
        setup_deploy_prj(&mut prj);
        let node = ZkSyncNode::start().await;
        let url = node.url();

        let private_key = ZkSyncNode::rich_wallets()
            .next()
            .map(|(_, pk, _)| pk)
            .expect("No rich wallets available");

        // Install required dependencies
        cmd.args([
            "install",
            "OpenZeppelin/openzeppelin-contracts",
            "cyfrin/zksync-contracts",
            "--shallow",
        ])
        .assert_success();
        cmd.forge_fuse();

        // Deploy the paymaster contract first
        let paymaster_deployment = cmd
            .forge_fuse()
            .args([
                "create",
                "./src/MyPaymaster.sol:MyPaymaster",
                "--rpc-url",
                url.as_str(),
                "--private-key",
                private_key,
                "--via-ir",
                "--value",
                "1000000000000000000",
                "--zksync",
                "--broadcast",
            ])
            .assert_success()
            .get_output()
            .stdout_lossy();

        // Extract the deployed paymaster address
        let re = regex::Regex::new(r"Deployed to: (0x[a-fA-F0-9]{40})").unwrap();
        let paymaster_address = re
            .captures(&paymaster_deployment)
            .and_then(|caps| caps.get(1))
            .map(|addr| addr.as_str())
            .expect("Failed to extract paymaster address");

        // Test successful deployment with valid paymaster input
        let greeter_deployment = cmd.forge_fuse()
        .args([
            "create",
            "./src/Greeter.sol:Greeter",
            "--rpc-url",
            url.as_str(),
            "--private-key",
            private_key,
            "--zk-paymaster-address",
            paymaster_address,
            "--zk-paymaster-input",
            "0x8c5a344500000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000000",
            "--via-ir",
            "--zksync",
            "--broadcast",
        ])
        .assert_success()
        .get_output()
        .stdout_lossy();

        // Verify successful deployment
        assert!(greeter_deployment.contains("Deployed to:"));

        // Test deployment failure with invalid paymaster input
        cmd.forge_fuse()
        .args([
            "create",
            "./src/Greeter.sol:Greeter",
            "--rpc-url",
            url.as_str(),
            "--private-key",
            private_key,
            "--zk-paymaster-address",
            paymaster_address,
            "--zk-paymaster-input",
            "0x0000000000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000000",
            "--via-ir",
            "--zksync",
            "--broadcast",
        ])
        .assert_failure();
    });

    forgetest_async!(paymaster_script_test, |prj, cmd| {
        setup_deploy_prj(&mut prj);
        cmd.forge_fuse();
        // We added the optimizer flag which is now false by default so we need to set it to true
        run_zk_script_test(
            prj.root(),
            &mut cmd,
            "./script/Paymaster.s.sol",
            "PaymasterScript",
            Some("OpenZeppelin/openzeppelin-contracts cyfrin/zksync-contracts"),
            3,
            Some(&["-vvvvv", "--via-ir", "--optimize", "true"]),
        )
        .await;
    });

    fn setup_deploy_prj(prj: &mut TestProject) {
        util::initialize(prj.root());
        prj.add_script("Paymaster.s.sol", include_str!("../../fixtures/zk/Paymaster.s.sol"));
        prj.add_source("MyPaymaster.sol", include_str!("../../fixtures/zk/MyPaymaster.sol"));
        prj.add_source("Greeter.sol", include_str!("../../../../../testdata_zk/Greeter.sol"));
    }
}

mod proxy {
    use super::*;

    forgetest_async!(script_zk_can_deploy_proxy, |prj, cmd| {
        setup_proxy_prj(&mut prj);
        run_zk_script_test(
            prj.root(),
            &mut cmd,
            "./script/Proxy.s.sol",
            "ProxyScript",
            Some("OpenZeppelin/openzeppelin-contracts"),
            4,
            Some(&["--broadcast"]),
        )
        .await;
    });

    fn setup_proxy_prj(prj: &mut TestProject) {
        util::initialize(prj.root());
        prj.add_script("Proxy.s.sol", include_str!("../../fixtures/zk/Proxy.s.sol"));
    }
}

mod script {
    use super::*;

    forgetest_async!(test_zk_can_broadcast_with_keystore_account, |prj, cmd| {
        util::initialize(prj.root());
        prj.add_script("Deploy.s.sol", include_str!("../../fixtures/zk/Deploy.s.sol"));
        prj.add_source("Greeter.sol", include_str!("../../../../../testdata_zk/Greeter.sol"));

        let node = ZkSyncNode::start().await;
        let url = node.url();

        cmd.forge_fuse();

        let script_path_contract = "./script/Deploy.s.sol:DeployScript";
        let keystore_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/zk/test_zksync_keystore");

        let script_args = vec![
            "--zk-startup",
            &script_path_contract,
            "--broadcast",
            "--keystores",
            keystore_path.to_str().unwrap(),
            "--password",
            "password",
            "--chain",
            "260",
            "--gas-estimate-multiplier",
            "310",
            "--rpc-url",
            url.as_str(),
            "--slow",
        ];

        cmd.arg("script").args(&script_args);

        cmd.assert_success()
            .get_output()
            .stdout_lossy()
            .contains("ONCHAIN EXECUTION COMPLETE & SUCCESSFUL");

        let run_latest = foundry_common::fs::json_files(prj.root().join("broadcast").as_path())
            .find(|file| file.ends_with("run-latest.json"))
            .expect("No broadcast artifacts");

        let content = foundry_common::fs::read_to_string(run_latest).unwrap();

        let json: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(json["transactions"].as_array().expect("broadcastable txs").len(), 3);
        cmd.forge_fuse();
    });
}
