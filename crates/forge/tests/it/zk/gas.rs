use foundry_test_utils::{forgetest_async, util, TestProject};

use foundry_test_utils::{util::OutputExt, ZkSyncNode};

forgetest_async!(script_execution_with_gas_price, |prj, cmd| {
    setup_gas_prj(&mut prj);

    let node = ZkSyncNode::start().await;
    let url = node.url();

    cmd.forge_fuse();

    let private_key =
        ZkSyncNode::rich_wallets().next().map(|(_, pk, _)| pk).expect("No rich wallets available");

    let script_args = vec![
        "--zk-startup",
        "./script/Gas.s.sol",
        "--private-key",
        &private_key,
        "--chain",
        "260",
        "--rpc-url",
        url.as_str(),
        "--slow",
        "--evm-version",
        "shanghai",
        "-vvvvv",
        "--broadcast",
        "--with-gas-price",
        "370000037",
        "--priority-gas-price",
        "123123",
    ];

    cmd.arg("script").args(&script_args);

    let stdout = cmd.assert_success().get_output().stdout_lossy();
    assert!(stdout.contains("ONCHAIN EXECUTION COMPLETE & SUCCESSFUL"));

    let run_latest = foundry_common::fs::json_files(prj.root().join("broadcast").as_path())
        .find(|file| file.ends_with("run-latest.json"))
        .expect("No broadcast artifacts");

    let json: serde_json::Value =
        serde_json::from_str(&foundry_common::fs::read_to_string(run_latest).unwrap()).unwrap();

    assert_eq!(json["transactions"].as_array().expect("broadcastable txs").len(), 1);

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

fn setup_gas_prj(prj: &mut TestProject) {
    util::initialize(prj.root());
    prj.add_script("Gas.s.sol", include_str!("../../fixtures/zk/Gas.s.sol")).unwrap();
    prj.add_source("Greeter.sol", include_str!("../../../../../testdata/zk/Greeter.sol")).unwrap();
}
