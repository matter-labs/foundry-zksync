//! Contains various tests for checking forge's commands in zksync

use foundry_config::Config;
use foundry_test_utils::util::OutputExt;
use similar_asserts::assert_eq;

forgetest!(test_zk_gas_report, |prj, cmd| {
    prj.insert_ds_test();
    prj.add_source(
        "Contracts.sol",
        r#"
//SPDX-license-identifier: MIT

import "./test.sol";

contract ContractOne {
    int public i;

    constructor() {
        i = 0;
    }

    function foo() public{
        while(i<5){
            i++;
        }
    }
}

contract ContractOneTest is DSTest {
    ContractOne c1;

    function setUp() public {
        c1 = new ContractOne();
    }

    function testFoo() public {
        c1.foo();
    }
}   
    "#,
    )
    .unwrap();

    prj.write_config(Config {
        gas_reports: (vec!["*".to_string()]),
        gas_reports_ignore: (vec![]),
        ..Default::default()
    });
    let out = cmd.arg("test").arg("--gas-report").assert_success().get_output().stdout_lossy();
    cmd.forge_fuse();
    let out_zk = cmd
        .arg("test")
        .arg("--gas-report")
        .arg("--zksync")
        .assert_success()
        .get_output()
        .stdout_lossy();

    let mut cells = out.split('|');
    let deployment_cost: u64 = cells.nth(17).unwrap().trim().parse().unwrap();
    let deployment_size: u64 = cells.next().unwrap().trim().parse().unwrap();
    let function = cells.nth(25).unwrap().trim();
    let gas: u64 = cells.next().unwrap().trim().parse().unwrap();

    let mut cells_zk = out_zk.split('|');
    let deployment_cost_zk: u64 = cells_zk.nth(17).unwrap().trim().parse().unwrap();
    let deployment_size_zk: u64 = cells_zk.next().unwrap().trim().parse().unwrap();
    let function_zk = cells_zk.nth(25).unwrap().trim();
    let gas_zk: u64 = cells_zk.next().unwrap().trim().parse().unwrap();

    assert!(deployment_cost_zk > deployment_cost);
    assert!(deployment_size_zk > deployment_size);
    assert!(gas_zk > gas);
    assert_eq!(function, "foo");
    assert_eq!(function_zk, "foo");
});

forgetest_init!(test_zk_can_init_with_zksync, |prj, cmd| {
    cmd.args(["init", "--zksync", "--force"]).assert_success();

    // Check that zkout/ is in .gitignore
    let gitignore_path = prj.root().join(".gitignore");
    assert!(gitignore_path.exists());
    let gitignore_contents = std::fs::read_to_string(&gitignore_path).unwrap();
    assert!(gitignore_contents.contains("zkout/"));

    // Assert that forge-zksync-std is installed
    assert!(prj.root().join("lib/forge-zksync-std").exists());
});

// Related to: https://github.com/matter-labs/foundry-zksync/issues/478
forgetest_async!(test_zk_can_detect_call_to_empty_contract, |prj, cmd| {
    foundry_test_utils::util::initialize(prj.root());

    prj.add_test(
        "CallEmptyCode.t.sol",
        r#"
import "forge-std/Test.sol";

// https://github.com/matter-labs/foundry-zksync/issues/478
contract CallEmptyCode is Test {
    // This test should make our EraVM tracer print out an ERROR trace
    function testDetectEmptyCodeContracts() external {
        address mockMe = address(123456789);

        vm.mockCall(mockMe, abi.encodeWithSignature("foo()"), abi.encode(42));

        (bool success, bytes memory ret) = mockMe.call(abi.encodeWithSignature("bar()"));

        require(!success, "callMethod succeeded when it should have failed");
        require(keccak256(ret) != keccak256(abi.encode(42)), "return expected to be different but it was the same");

    }
}
"#,
    )
    .unwrap();
    cmd.args(["test", "--zksync", "--evm-version", "shanghai", "--mc", "CallEmptyCode"]);

    cmd.assert_success()
        .get_output()
        .stdout_lossy()
        .contains("call may fail or behave unexpectedly due to empty code");
});

forgetest_async!(test_zk_can_send_eth_to_eoa_without_warnings, |prj, cmd| {
    foundry_test_utils::util::initialize(prj.root());
    prj.add_test(
        "SendEthToEOA.t.sol",
        r#"
import "forge-std/Test.sol";

contract SendEthToEOA is Test {
    function testSendEthToEOA() external {
        address eoa = makeAddr("Juan's Account");
        vm.deal(address(this), 1 ether);
        
        (bool success,) = eoa.call{value: 1 ether}("");
        assertTrue(success, "ETH transfer failed");
    }
}
"#,
    )
    .unwrap();

    cmd.args(["test", "--zksync", "--match-test", "testSendEthToEOA"]);
    let output = cmd.assert_success().get_output().stdout_lossy();

    assert!(!output.contains("call may fail or behave unexpectedly due to empty code"));
});

forgetest_async!(test_zk_calling_empty_code_with_zero_value_issues_warning, |prj, cmd| {
    foundry_test_utils::util::initialize(prj.root());
    prj.add_test(
        "CallEmptyCodeWithZeroValue.t.sol",
        r#"
import "forge-std/Test.sol";

contract CallEmptyCodeWithZeroValue is Test {
    function testCallEmptyCodeWithZeroValue() external {
        address eoa = makeAddr("Juan's Account");
        vm.deal(address(this), 1 ether);
        
        (bool success,) = eoa.call("");
        assertTrue(success, "call failed");
    }
}
"#,
    )
    .unwrap();

    cmd.args(["test", "--zksync", "--match-test", "testCallEmptyCodeWithZeroValue"]);
    let output = cmd.assert_success().get_output().stdout_lossy();

    assert!(output.contains("call may fail or behave unexpectedly due to empty code"));
});
