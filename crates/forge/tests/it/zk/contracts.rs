//! Forge tests for zksync contracts.

use crate::{config::*, test_helpers::TEST_DATA_DEFAULT};
use forge::revm::primitives::SpecId;
use foundry_config::fs_permissions::PathPermission;
use foundry_test_utils::{util, Filter};

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_contract_can_call_function() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new(
        "testZkContractCanCallMethod|testZkContractsMultipleTransactions",
        "ZkContractsTest",
        ".*",
    );

    TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_contract_persisted_contracts_after_fork() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testZkContractsPersistedDeployedContractNoArgs|testZkContractsPersistedDeployedContractArgs", "ZkContractsTest", ".*");

    TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_contract_deployment() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testZkContractsInlineDeployedContractNoArgs|testZkContractsInlineDeployedContractComplexArgs", "ZkContractsTest", ".*");

    TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_contract_deployment_balance() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter =
        Filter::new("testZkContractsInlineDeployedContractBalance", "ZkContractsTest", ".*");

    TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_contract_deployment_balance_transfer() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testZkContractsExpectedBalances", "ZkContractsTest", ".*");

    TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_contract_create2() {
    let (prj, mut cmd) = util::setup_forge("test_zk_contract_create2_with_deps", foundry_test_utils::foundry_compilers::PathStyle::Dapptools);
    util::initialize(prj.root());

    cmd.args(["install", "matter-labs/era-contracts", "--no-commit", "--shallow"]).ensure_execute_success().expect("able to install dependencies");
    cmd.forge_fuse();

    let mut config = cmd.config();
    config.fs_permissions.add(PathPermission::read("./zkout"));
    prj.write_config(config);

    prj.add_source("Greeter.sol", include_str!("../../../../../testdata/zk/Greeter.sol")).unwrap();

    prj.add_source("CustomNumber.sol", include_str!("../../../../../testdata/zk/CustomNumber.sol")).unwrap();

    prj.add_source("Create2Utils.sol", include_str!("../../../../../testdata/zk/Create2Utils.sol")).unwrap();

    prj.add_test("Create2.t.sol",
    r#"
pragma solidity ^0.8.18;
import "forge-std/Test.sol";
import {L2ContractHelper} from "era-contracts/l2-contracts/contracts/L2ContractHelper.sol"; // =0.8.20

import {Greeter} from "../src/Greeter.sol";
import {CustomNumber} from "../src/CustomNumber.sol";

import {Create2Utils} from "../src/Create2Utils.sol";

contract Create2Test is Test {
    function getBytecodeHash(string memory path) internal returns (bytes32 bytecodeHash) {
        string memory artifact = vm.readFile(path);
        bytecodeHash = vm.parseJsonBytes32(
            artifact,
            '.hash'
        );
    }

    function testCanDeployViaCreate2() public {
        bytes32 bytecodeHash = getBytecodeHash("zkout/Greeter.sol/Greeter.json");
        address sender = address(0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496);
        bytes32 salt = "12345";
        bytes32 constructorInputHash = keccak256(abi.encode());

        address expectedAddress =
            Create2Utils.computeCreate2Address(sender, salt, bytes32(bytecodeHash), constructorInputHash);

        // deploy via create2
        address actualAddress = address(new ConstantNumber{salt: salt}());

        assertEq(actualAddress, expectedAddress);
    }


    function testComputeCreate2WithNoArgs() external {
        bytes32 salt = bytes32(0x0);

        bytes32 bytecodeHash = getBytecodeHash("zkout/Greeter.sol/Greeter.json");

        address computedAddress = Create2Utils.computeCreate2Address(
            address(this),
            salt,
            bytes32(bytecodeHash),
            keccak256(abi.encode())
        );
        address expectedAddress = L2ContractHelper.computeCreate2Address(
            address(this),
            salt,
            bytes32(bytecodeHash),
            keccak256(abi.encode())
        );

        address actualAddress = address(new Greeter{salt: salt}());
        assertEq(actualAddress, expectedAddress);
        assertEq(computedAddress, expectedAddress);
    }

    function testComputeCreate2WithArgs() external {
        bytes32 salt = bytes32(0x0);
        uint8 value = 42;

        bytes32 bytecodeHash = getBytecodeHash("zkout/CustomNumber.sol/CustomNumber.json");

        address computedAddress = Create2Utils.computeCreate2Address(
            address(this),
            salt,
            bytecodeHash,
            keccak256(abi.encode(value))
        );
        address expectedAddress = L2ContractHelper.computeCreate2Address(
            address(this),
            salt,
            bytecodeHash,
            keccak256(abi.encode(value))
        );

        CustomNumber num = new CustomNumber{salt: salt}(value);
        assertEq(address(num), expectedAddress);
        assertEq(computedAddress, expectedAddress);
        assertEq(num.number(), value);
    }
}
"#).unwrap();

    cmd.args(["test", "--zk-startup", "--mc", "Create2Test"]);
    assert!(cmd.stdout_lossy().contains("Suite result: ok"));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_contract_can_call_system_contracts() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testZkContractsCallSystemContract", "ZkContractsTest", ".*");

    TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_contract_deployed_in_setup_can_be_mocked() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testZkContractsDeployedInSetupAreMockable", "ZkContractsTest", ".*");

    TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).run().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_contract_static_calls_keep_nonce_consistent() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    let filter = Filter::new("testZkStaticCalls", "ZkContractsTest", ".*");

    TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).run().await;
}
