//! Contains tests related to `forge script` with zksync.

use foundry_test_utils::util::OutputExt;
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
