use foundry_test_utils::{
    ZkSyncNode, forgetest_async,
    util::{self, OutputExt},
};

use crate::test_helpers::deploy_zk_contract;

forgetest_async!(forge_zk_can_deploy_erc20, |prj, cmd| {
    util::initialize(prj.root());
    prj.add_source("ERC20.sol", include_str!("../../../../../testdata/zk/ERC20.sol")).unwrap();

    let node = ZkSyncNode::start().await;
    let url = node.url();

    let private_key =
        ZkSyncNode::rich_wallets().next().map(|(_, pk, _)| pk).expect("No rich wallets available");

    let erc20_address =
        deploy_zk_contract(&mut cmd, url.as_str(), private_key, "./src/ERC20.sol:MyToken", None)
            .expect("Failed to deploy ERC20 contract");

    assert!(!erc20_address.is_empty(), "Deployed address should not be empty");
});

forgetest_async!(forge_zk_can_deploy_contracts_and_cast_a_transaction, |prj, cmd| {
    util::initialize(prj.root());
    prj.add_source(
        "TokenReceiver.sol",
        include_str!("../../../../../testdata/zk/TokenReceiver.sol"),
    )
    .unwrap();
    prj.add_source("ERC20.sol", include_str!("../../../../../testdata/zk/ERC20.sol")).unwrap();

    let node = ZkSyncNode::start().await;
    let url = node.url();

    let private_key =
        ZkSyncNode::rich_wallets().next().map(|(_, pk, _)| pk).expect("No rich wallets available");

    let token_receiver_address = deploy_zk_contract(
        &mut cmd,
        url.as_str(),
        private_key,
        "./src/TokenReceiver.sol:TokenReceiver",
        None,
    )
    .expect("Failed to deploy TokenReceiver contract");
    let erc_20_address =
        deploy_zk_contract(&mut cmd, url.as_str(), private_key, "./src/ERC20.sol:MyToken", None)
            .expect("Failed to deploy ERC20 contract");

    cmd.cast_fuse().args([
        "send",
        "--rpc-url",
        url.as_str(),
        "--private-key",
        private_key,
        &erc_20_address,
        "transfer(address,uint256)",
        &token_receiver_address,
        "1",
    ]);

    let stdout = cmd.assert_success().get_output().stdout_lossy();

    assert!(stdout.contains("transactionHash"), "Transaction hash not found in output");
    assert!(stdout.contains("success"), "Transaction was not successful");
});

forgetest_async!(forge_zk_can_deploy_contracts_with_gas_per_pubdata_and_succeed, |prj, cmd| {
    util::initialize(prj.root());
    prj.add_source("ERC20.sol", include_str!("../../../../../testdata/zk/ERC20.sol")).unwrap();

    let node = ZkSyncNode::start().await;
    let url = node.url();

    let private_key =
        ZkSyncNode::rich_wallets().next().map(|(_, pk, _)| pk).expect("No rich wallets available");

    deploy_zk_contract(
        &mut cmd,
        url.as_str(),
        private_key,
        "./src/ERC20.sol:MyToken",
        Some(&["--zk-gas-per-pubdata", "3000"]),
    )
    .expect("Failed to deploy ERC20 contract");
});

forgetest_async!(
    forge_zk_can_deploy_contracts_with_invalid_gas_per_pubdata_and_fail,
    |prj, cmd| {
        util::initialize(prj.root());
        prj.add_source("ERC20.sol", include_str!("../../../../../testdata/zk/ERC20.sol")).unwrap();

        let node = ZkSyncNode::start().await;
        let url = node.url();

        let private_key = ZkSyncNode::rich_wallets()
            .next()
            .map(|(_, pk, _)| pk)
            .expect("No rich wallets available");
        cmd.forge_fuse().args([
            "create",
            "--zk-startup",
            "./src/ERC20.sol:MyToken",
            "--rpc-url",
            url.as_str(),
            "--private-key",
            private_key,
            "--broadcast",
            "--timeout",
            "1",
            "--zk-gas-per-pubdata",
            "1",
        ]);

        cmd.assert_failure();
    }
);

forgetest_async!(forge_zk_create_dry_run_without_broadcast, |prj, cmd| {
    util::initialize(prj.root());
    prj.add_source("ERC20.sol", include_str!("../../../../../testdata/zk/ERC20.sol")).unwrap();

    let node = ZkSyncNode::start().await;
    let url = node.url();

    let private_key =
        ZkSyncNode::rich_wallets().next().map(|(_, pk, _)| pk).expect("No rich wallets available");

    // Test dry-run behavior WITHOUT --broadcast flag
    cmd.forge_fuse().args([
        "create",
        "--zk-startup",
        "./src/ERC20.sol:MyToken",
        "--rpc-url",
        url.as_str(),
        "--private-key",
        private_key,
    ]);

    let output = cmd.assert_success().get_output().stdout_lossy();

    // Should show transaction details (this proves dry-run is working)
    assert!(output.contains("Contract: MyToken"), "Expected contract name in output");
    assert!(output.contains("Transaction:"), "Expected transaction details in output");
    assert!(output.contains("ABI:"), "Expected ABI output in output");

    // Should NOT show deployment success messages (this is the key test)
    assert!(!output.contains("Deployed to:"), "Should not show deployment address in dry-run");
    assert!(!output.contains("Transaction hash:"), "Should not show transaction hash in dry-run");
});

forgetest_async!(forge_zk_create_with_broadcast_flag, |prj, cmd| {
    util::initialize(prj.root());
    prj.add_source("ERC20.sol", include_str!("../../../../../testdata/zk/ERC20.sol")).unwrap();

    let node = ZkSyncNode::start().await;
    let url = node.url();

    let private_key =
        ZkSyncNode::rich_wallets().next().map(|(_, pk, _)| pk).expect("No rich wallets available");

    // Test actual deployment WITH --broadcast flag (helper function already includes --broadcast)
    let deployed_address =
        deploy_zk_contract(&mut cmd, url.as_str(), private_key, "./src/ERC20.sol:MyToken", None)
            .expect("Failed to deploy ERC20 contract");

    // Should successfully deploy and return an address
    assert!(!deployed_address.is_empty(), "Deployed address should not be empty");
    assert!(deployed_address.starts_with("0x"), "Address should be a valid hex address");
});
