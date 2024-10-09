//! Forge tests for zksync factory contracts.

use forge::revm::primitives::SpecId;
use foundry_test_utils::{
    forgetest_async,
    util::{self, OutputExt},
    Filter, ZkSyncNode,
};

use crate::{config::TestConfig, test_helpers::TEST_DATA_DEFAULT};

#[tokio::test(flavor = "multi_thread")]
#[ignore = "disabled since #476"]
async fn test_zk_can_deploy_large_factory_deps() {
    let runner = TEST_DATA_DEFAULT.runner_zksync();
    {
        let filter = Filter::new(".*", "ZkLargeFactoryDependenciesTest", ".*");
        TestConfig::with_filter(runner, filter).evm_spec(SpecId::SHANGHAI).run().await;
    }
}

forgetest_async!(
    #[ignore = "disabled since #476"]
    script_zk_can_deploy_large_factory_deps,
    |prj, cmd| {
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

        let node = ZkSyncNode::start();

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

        let json: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(json["transactions"].as_array().expect("broadcastable txs").len(), 1);
    }
);
