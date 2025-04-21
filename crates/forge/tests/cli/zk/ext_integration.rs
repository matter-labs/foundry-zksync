use foundry_test_utils::util::ExtTester;

#[test]
fn test_zk_aave_di() {
    ExtTester::new("Moonsong-Labs", "aave-delivery-infrastructure", "ci")
        .args(["--zksync", "--skip", "\"*/PayloadScripts.t.sol\""])
        .run()
}

#[test]
#[ignore = "uses `EXTCODECOPY` in multiple tests"]
fn test_zk_zkemail_email_recovery() {
    ExtTester::new("zkemail", "email-recovery", "main")
        .args(["--zksync", "--root", "."])
        // repo uses 0.5.2, affected by:
        // https://github.com/rhinestonewtf/modulekit/issues/181
        .install_command(&["npm", "install", "@rhinestone/modulekit@0.5.8"])
        .run()
}

#[test]
#[ignore = "computed CREATE2 address differs from zksync's"]
fn test_zk_zkemail_email_signer() {
    ExtTester::new("zkemail", "email-signer", "main")
        .args(["--zksync", "--root", "contracts"])
        .install_command(&["yarn", "install"])
        .run()
}

#[test]
#[ignore = "only contains scripts"]
fn test_zk_zkemail_email_wallet_contracts() {
    ExtTester::new("zkemail", "email-wallet-contracts", "main")
        .args(["--zksync", "--root", "."])
        .install_command(&["yarn", "install"])
        .run()
}

#[test]
#[ignore = "uses EXTCODECOPY in `EmailApprover.t.sol`"]
fn test_zk_zkemail_email_approver() {
    ExtTester::new("zkemail", "email-approver", "main")
        .args(["--zksync", "--root", "packages/contracts"])
        .install_command(&["yarn", "install"])
        .run()
}

#[test]
#[ignore = "only contains scripts"]
fn test_zk_zkemail_email_tx_builder_template() {
    ExtTester::new("zkemail", "email-tx-builder-template", "main")
        .args(["--zksync", "--root", "contracts"])
        .install_command(&["yarn", "install"])
        .run()
}

#[test]
#[ignore = "uses CODECOPY in `EmailWalletCore.sol`"]
fn test_zk_zkemail_email_wallet() {
    ExtTester::new("zkemail", "email-wallet", "main")
        .args([
            "--zksync",
            "--root",
            "packages/contracts",
            "--zk-suppressed-errors",
            "sendtransfer",
        ])
        .install_command(&["yarn", "install"])
        .run()
}

#[test]
#[ignore = "has outdated yarn.lock, fails to fetch deps"]
fn test_zk_zkemail_proof_of_twitter() {
    ExtTester::new("zkemail", "proof-of-twitter", "main")
        .args(["--zksync", "--root", "packages/contracts"])
        .install_command(&["yarn", "install"])
        .run()
}

#[test]
fn test_zk_zkemail_zk_email_verify() {
    ExtTester::new("zkemail", "zk-email-verify", "main")
        .args(["--zksync", "--root", "packages/contracts", "--nmt", "testFail"])
        .install_command(&["yarn", "install"])
        .run()
}

#[test]
fn test_zk_zkemail_email_tx_builder() {
    ExtTester::new("zkemail", "email-tx-builder", "main")
        .args([
            "--zksync",
            "--root",
            "packages/contracts",
            "--nmt",
            "testFail",
            "--nmc",
            // computes differing CREATE2 addresses than zksync
            "EmailSignerFactory",
            "--nmp",
            // affected by https://github.com/matter-labs/foundry-zksync/issues/987
            "StringUtils",
        ])
        .install_command(&["yarn", "install"])
        .run()
}

#[test]
fn test_zk_zkemail_jwt_tx_builder() {
    ExtTester::new("zkemail", "jwt-tx-builder", "main")
        .args([
            "--zksync",
            "--root",
            "packages/contracts",
            "--nmt",
            // public.json generation fails
            "testFail|verifyEmailProof",
        ])
        .install_command(&["yarn", "install"])
        .run()
}
