use foundry_test_utils::{
    forgetest_async,
    util::{self, OutputExt},
    ZkSyncNode,
};
use std::path::Path;

forgetest_async!(test_zk_can_broadcast_with_keystore_account, |prj, cmd| {
    util::initialize(prj.root());
    prj.add_script("Deploy.s.sol", include_str!("../../fixtures/zk/Deploy.s.sol")).unwrap();
    prj.add_source("Greeter.sol", include_str!("../../../../../testdata/zk/Greeter.sol")).unwrap();

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
