//! Forge tests for zkysnc issues, to avoid regressions.
//!
//! Issue list: https://github.com/matter-labs/foundry-zksync/issues

use crate::{
    config::*,
    repros::test_repro,
    test_helpers::{ForgeTestData, TEST_DATA_DEFAULT},
};
use alloy_primitives::Address;
use foundry_config::{fs_permissions::PathPermission, FsPermissions};
use foundry_test_utils::Filter;

//zk-specific repros configuration
async fn repro_config(
    issue: usize,
    should_fail: bool,
    sender: Option<Address>,
    test_data: &ForgeTestData,
) -> TestConfig {
    foundry_test_utils::init_tracing();
    let filter = Filter::path(&format!(".*repros/Issue{issue}.t.sol"));

    let mut config = test_data.config.clone();
    config.fs_permissions = FsPermissions::new(vec![
        PathPermission::read("./fixtures/zk"),
        PathPermission::read("zkout"),
    ]);
    if let Some(sender) = sender {
        config.sender = sender;
    }

    let runner = test_data.runner_with_zksync_config(config);
    TestConfig::with_filter(runner, filter).set_should_fail(should_fail)
}

// https://github.com/matter-labs/foundry-zksync/issues/497
test_repro!(497);

// https://github.com/matter-labs/foundry-zksync/issues/478
foundry_test_utils::forgetest_async!(issue_478, |prj, cmd| {
    foundry_test_utils::util::initialize(prj.root());

    prj.add_test("Issue478.t.sol",
                 r#"
import "forge-std/Test.sol";

// https://github.com/matter-labs/foundry-zksync/issues/478
contract Issue478 is Test {
    // This test should make our EraVM tracer print out an ERROR trace
    function testFailDetectEmptyCodeContracts() external {
        address mockMe = address(123456789);

        vm.mockCall(mockMe, abi.encodeWithSignature("foo()"), abi.encode(42));

        (bool success, bytes memory ret) = mockMe.call(abi.encodeWithSignature("bar()"));

        require(success, "callMethod failed");
        require(keccak256(ret) == keccak256(abi.encode(42)), "return not as expected");
    }
}
"#).unwrap();
    cmd.args(["test", "--zksync", "--evm-version", "shanghai"]);

    let output = cmd.stdout_lossy();
    assert!(output.contains("call may fail or behave unexpectedly due to empty code"));
});
