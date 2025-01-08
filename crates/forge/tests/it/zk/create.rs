use foundry_test_utils::{
    forgetest_async,
    util::{self, OutputExt},
    ZkSyncNode,
};

use crate::test_helpers::deploy_zk_contract;

forgetest_async!(forge_zk_can_deploy_erc20, |prj, cmd| {
    util::initialize(prj.root());
    prj.add_source("ERC20.sol", include_str!("../../../../../testdata/zk/ERC20.sol")).unwrap();

    let node = ZkSyncNode::start();
    let url = node.url();

    let private_key =
        ZkSyncNode::rich_wallets().next().map(|(_, pk, _)| pk).expect("No rich wallets available");

    let erc20_address =
        deploy_zk_contract(&mut cmd, url.as_str(), private_key, "./src/ERC20.sol:MyToken")
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

    let node = ZkSyncNode::start();
    let url = node.url();

    let private_key =
        ZkSyncNode::rich_wallets().next().map(|(_, pk, _)| pk).expect("No rich wallets available");

    let token_receiver_address = deploy_zk_contract(
        &mut cmd,
        url.as_str(),
        private_key,
        "./src/TokenReceiver.sol:TokenReceiver",
    )
    .expect("Failed to deploy TokenReceiver contract");
    let erc_20_address =
        deploy_zk_contract(&mut cmd, url.as_str(), private_key, "./src/ERC20.sol:MyToken")
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
