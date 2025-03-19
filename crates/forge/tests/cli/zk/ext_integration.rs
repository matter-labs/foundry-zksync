use foundry_test_utils::util::ExtTester;

#[test]
fn test_zk_aave_di() {
    ExtTester::new("Moonsong-Labs", "aave-delivery-infrastructure", "ci")
        .args(["--zksync", "--skip", "\"*/PayloadScripts.t.sol\""])
        .run()
}

eval_macro::eval! {
    let repos = [
        ("email-recovery", "."),
        ("email-tx-builder", "packages/contracts"),
        ("zk-email-verify", "packages/contracts"),
        ("email-signer", "contracts"),
        ("jwt-tx-builder", "packages/contracts"),
        ("email-wallet", "packages/contracts"),
        ("email-wallet-contracts", "."),
        ("email-approver", "packages/contracts"),
        ("proof-of-twitter", "packages/contracts"),
        ("email-tx-builder-template", "contracts"),
    ];

    for (name, path) in repos {
        let snake_name = name.replace('-', "_");
        output! {
            #[test]
            fn test_zk_zkemail_{{snake_name}}() {
                ExtTester::new("zkemail", "{name}", "main")
                    .args(["--zksync", "--root", "{path}"])
                    .install_command(&["pnpm", "install", "--prefer-offline"])
                    .install_command(&["npm", "install", "--prefer-offline"])
                    .run()
            }
        }
    }
}
