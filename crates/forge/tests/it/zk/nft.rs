use foundry_test_utils::{forgetest_async, util, TestProject};

use crate::test_helpers::run_zk_script_test;

forgetest_async!(script_zk_can_deploy_nft, |prj, cmd| {
    setup_nft_prj(&mut prj);
    run_zk_script_test(
        prj.root(),
        &mut cmd,
        "./script/NFT.s.sol",
        "MyScript",
        Some("transmissions11/solmate@v7 OpenZeppelin/openzeppelin-contracts"),
        1,
        Some(&["-vvvvv", "--broadcast"]),
    )
    .await;
});

fn setup_nft_prj(prj: &mut TestProject) {
    util::initialize(prj.root());
    prj.add_script("NFT.s.sol", include_str!("../../fixtures/zk/NFT.s.sol")).unwrap();
}
