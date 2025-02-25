//! Forge tests for zksync factory contracts.

use forge::revm::primitives::SpecId;
use foundry_test_utils::{
    forgetest_async,
    util::{self, OutputExt},
    Filter, ZkSyncNode,
};
use foundry_zksync_core::utils::MAX_L2_GAS_LIMIT;

use crate::{config::TestConfig, test_helpers::TEST_DATA_DEFAULT};

#[tokio::test(flavor = "multi_thread")]
async fn test_zk_can_deploy_large_factory_deps() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    {
        let filter = Filter::new(".*", "ZkLargeFactoryDependenciesTest", ".*");
        TestConfig::with_filter(runner, filter).spec_id(SpecId::SHANGHAI).run().await;
    }
}

forgetest_async!(script_zk_can_deploy_large_factory_deps, |prj, cmd| {
    util::initialize(prj.root());

    prj.add_source(
        "LargeContracts.sol",
        include_str!("../../../../../testdata/zk/LargeContracts.sol"),
    )
    .unwrap();
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
    )
    .unwrap();

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
