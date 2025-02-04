use foundry_test_utils::{forgetest_async, util, TestProject};

use foundry_test_utils::{util::OutputExt, ZkSyncNode};

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
    let insufficient_multiplier_args =
        create_script_args(&private_key, &url, "--gas-estimate-multiplier", "1");
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
    // We had to change the approach of testing an invalid gas per pubdata value because there were
    // changes upstream for the timeout and retries mechanism Now we execute the command
    // directly and check the output with a manual timeout. The previous approach was to use the
    // `forge script` command with a timeout but now it's not timeouting anymore for this error.
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
    prj.add_script("Gas.s.sol", include_str!("../../fixtures/zk/Gas.s.sol")).unwrap();
    prj.add_source("Greeter.sol", include_str!("../../../../../testdata/zk/Greeter.sol")).unwrap();
}
