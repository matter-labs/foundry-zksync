//! Contains tests related to `forge script` with zksync.

use foundry_test_utils::{util::OutputExt, ZkSyncNode};
use foundry_zksync_core::ZkTransactionMetadata;

forgetest_async!(test_zk_can_execute_script_with_arguments, |prj, cmd| {
    #[derive(serde::Deserialize, Debug)]
    #[allow(dead_code)]
    struct ZkTransactions {
        transactions: Vec<ZkTransaction>,
    }

    #[derive(serde::Deserialize, Debug)]
    #[allow(dead_code)]
    struct ZkTransaction {
        transaction: ZkTransactionInner,
    }

    #[derive(serde::Deserialize, Debug)]
    #[allow(dead_code)]
    struct ZkTransactionInner {
        zksync: ZkTransactionMetadata,
    }

    let node = foundry_test_utils::ZkSyncNode::start().await;

    cmd.args(["init", "--force"]).arg(prj.root());
    cmd.assert_success();
    cmd.forge_fuse();

    prj.add_script(
        "Deploy.s.sol",
        r#"
pragma solidity ^0.8.18;

import {Script} from "forge-std/Script.sol";

contract Greeter {
    string name;
    uint256 age;

    event Greet(string greet);

    function greeting(string memory _name) public returns (string memory) {
        name = _name;
        string memory greet = string(abi.encodePacked("Hello ", _name));
        emit Greet(greet);
        return greet;
    }

    function setAge(uint256 _age) public {
        age = _age;
    }

    function getAge() public view returns (uint256) {
        return age;
    }
}

contract DeployScript is Script {
    Greeter greeter;
    string greeting;

    function run() external {
        // test is using old Vm.sol interface, so we call manually
        address(vm).call(abi.encodeWithSignature("zkVm(bool)", true));
        
        vm.startBroadcast();
        greeter = new Greeter();
        greeter.greeting("john");
        greeter.setAge(123);
        vm.stopBroadcast();
    }
}
   "#,
    )
    .unwrap();

    cmd.arg("script").args([
        "--zksync",
        "DeployScript",
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
        "--evm-version",
        "shanghai",
    ]);

    cmd.assert_success()
        .get_output()
        .stdout_lossy()
        .contains("ONCHAIN EXECUTION COMPLETE & SUCCESSFUL");

    let run_latest = foundry_common::fs::json_files(prj.root().join("broadcast").as_path())
        .find(|file| file.ends_with("run-latest.json"))
        .expect("No broadcast artifacts");

    let content = foundry_common::fs::read_to_string(run_latest).unwrap();

    let transactions: ZkTransactions = serde_json::from_str(&content).unwrap();
    let transactions = transactions.transactions;
    assert_eq!(transactions.len(), 3);
});

// <https://github.com/foundry-rs/foundry/issues/8993>
forgetest_async!(test_zk_broadcast_raw_create2_deployer, |prj, cmd| {
    foundry_test_utils::util::initialize(prj.root());
    let node = ZkSyncNode::start().await;
    let url = node.url();

    println!("1. url: {:?}", url);

    let (_, private_key) = ZkSyncNode::rich_wallets()
        .next()
        .map(|(addr, pk, _)| (addr, pk))
        .expect("No rich wallets available");

    //print private key
    println!("1. private_key: {:?}", private_key);

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
    )
    .unwrap();

    //deploy
    let output = cmd.args([
        "create",
        "src/Counter.sol:Counter",
        "--zksync",
        "--private-key",
        private_key,
        "--rpc-url",
        &url,
    ]);
    let address = output.assert_success().get_output().stdout_lossy();
    println!("2. address: {:?}", address.trim());
    cmd.forge_fuse();

    prj.add_script(
        "Foo",
        r#"
import "forge-std/Script.sol";
import {Counter} from "../src/Counter.sol";
contract SimpleScript is Script {
    function run() external {
        // zk raw transaction
        vm.startBroadcast();
        // This raw transaction comes from cast mktx of increment() to Counter contract
        vm.broadcastRawTransaction(
            hex"71f88580808402b275d08304d718949c1a3d7c98dbf89c7f5d167f2219c29c2fe775a78084d09de08a80a0f76c089059f46bb90adc0b34fa643edf175413b9e076185f46afe84f9283ccfba077b2a9878429569852f9e114e0c2bd8e67be25048752c3b2acab6cd7e2cdf4ff82010494bc989fde9e54cad2ab4392af6df60f04873a033a80c08080"
        );
        vm.stopBroadcast();
    }
}
"#,
    )
    .unwrap();

    println!("4. prj.root(): {:?}", prj.root());

    cmd.args([
        "script",
        "--zksync",
        "--private-key",
        private_key,
        "--rpc-url",
        &url,
        "--broadcast",
        "--slow",
        "--non-interactive",
        "SimpleScript",
    ]);

    println!("5. cmd: {:?}", "ok");

    let output = cmd.assert_success().get_output().stdout_lossy();
    println!("6. output: {:?}", output);

    assert!(output.contains("ONCHAIN EXECUTION COMPLETE & SUCCESSFUL."));
});
