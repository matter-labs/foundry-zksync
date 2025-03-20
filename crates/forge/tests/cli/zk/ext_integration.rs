use foundry_test_utils::util::ExtTester;

#[test]
fn test_zk_aave_di() {
    ExtTester::new("Moonsong-Labs", "aave-delivery-infrastructure", "ci")
        .args(["--zksync", "--skip", "\"*/PayloadScripts.t.sol\""])
        .run()
}

eval_macro::eval! {
    let repos: &[(&[&str], &str, &str)] = &[
        (&[r#"#[ignore = "https://github.com/rhinestonewtf/modulekit/issues/181"]"#], "email-recovery", "."),
        (&[r#"#[ignore = "computed CREATE2 address differs from zksync's"]"#], "email-signer", "contracts"),
        (&[r#"#[ignore = "only contains scripts"]"#], "email-wallet-contracts", "."),
        (&[r#"#[ignore = "uses EXTCODECOPY in EmailApprover.t.sol"]"#], "email-approver", "packages/contracts"),
        (&[], "jwt-tx-builder", "packages/contracts"),
        (&[], "email-wallet", "packages/contracts"),
        (&[], "proof-of-twitter", "packages/contracts"),
        (&[], "email-tx-builder-template", "contracts"),
    ];

    for (attrs, name, path) in repos {
        let snake_name = name.replace('-', "_");
        let attrs = format!("{}", attrs.join("\n"));
        output! {
            {{attrs}}
            #[test]
            fn test_zk_zkemail_{{snake_name}}() {
                ExtTester::new("zkemail", "{name}", "main")
                    .args(["--zksync", "--root", "{path}"])
                    .install_command(&["yarn", "install"])
                    // .install_command(&["pnpm", "install", "--prefer-offline"])
                    // .install_command(&["npm", "install", "--prefer-offline"])
                    .run()
            }
        }
    }
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
            // unfortunately library fails consistently
            "StringUtils",
        ])
        .install_command(&["yarn", "install"])
        .run()
}
