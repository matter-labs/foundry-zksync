use crate::utils::generate_large_contract;
use foundry_config::{Config, ZkSyncConfig};
use foundry_test_utils::{forgetest, snapbox::IntoData, str, util::OutputExt};
use globset::Glob;
use regex::Regex;

forgetest_init!(can_parse_build_filters, |prj, cmd| {
    prj.clear();

    cmd.args(["build", "--names", "--skip", "tests", "scripts"]).assert_success().stdout_eq(str![
        [r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!
  compiler version: [..]
    - Counter

"#]
    ]);
});

forgetest!(throws_on_conflicting_args, |prj, cmd| {
    prj.clear();

    cmd.args(["compile", "--format-json", "--quiet"]).assert_failure().stderr_eq(str![[r#"
error: the argument '--json' cannot be used with '--quiet'

Usage: forge[..] build --json [PATHS]...

For more information, try '--help'.

"#]]);
});

// tests that json is printed when --format-json is passed
forgetest!(compile_json, |prj, cmd| {
    prj.add_source(
        "jsonError",
        r"
contract Dummy {
    uint256 public number;
    function something(uint256 newNumber) public {
        number = newnumber; // error here
    }
}
",
    )
    .unwrap();

    // set up command
    cmd.args(["compile", "--format-json"]).assert_success().stdout_eq(str![[r#"
{
  "errors": [
    {
      "sourceLocation": {
        "file": "src/jsonError.sol",
        "start": 184,
        "end": 193
      },
      "type": "DeclarationError",
      "component": "general",
      "severity": "error",
      "errorCode": "7576",
      "message": "Undeclared identifier. Did you mean \"newNumber\"?",
      "formattedMessage": "DeclarationError: Undeclared identifier. Did you mean \"newNumber\"?\n [FILE]:7:18:\n  |\n7 |         number = newnumber; // error here\n  |                  ^^^^^^^^^\n\n"
    }
  ],
  "sources": {},
  "contracts": {},
  "build_infos": "{...}"
}
"#]].is_json());
});

forgetest!(initcode_size_exceeds_limit, |prj, cmd| {
    prj.add_source("LargeContract", generate_large_contract(5450).as_str()).unwrap();
    cmd.args(["build", "--sizes"]).assert_failure().stdout_eq(str![
        r#"
...
| Contract     | Runtime Size (B) | Initcode Size (B) | Runtime Margin (B) | Initcode Margin (B) |
|--------------|------------------|-------------------|--------------------|---------------------|
| HugeContract |              194 |            49,344 |             24,382 |                -192 |
...
"#
    ]);

    cmd.forge_fuse().args(["build", "--sizes", "--json"]).assert_failure().stdout_eq(
        str![[r#"
{
   "HugeContract":{
      "runtime_size":194,
      "init_size":49344,
      "runtime_margin":24382,
      "init_margin":-192
   }
}
"#]]
        .is_json(),
    );
});

forgetest!(initcode_size_limit_can_be_ignored, |prj, cmd| {
    prj.add_source("LargeContract", generate_large_contract(5450).as_str()).unwrap();
    cmd.args(["build", "--sizes", "--ignore-eip-3860"]).assert_success().stdout_eq(str![
        r#"
...
| Contract     | Runtime Size (B) | Initcode Size (B) | Runtime Margin (B) | Initcode Margin (B) |
|--------------|------------------|-------------------|--------------------|---------------------|
| HugeContract |              194 |            49,344 |             24,382 |                -192 |
...
"#
    ]);

    cmd.forge_fuse()
        .args(["build", "--sizes", "--ignore-eip-3860", "--json"])
        .assert_success()
        .stdout_eq(
            str![[r#"
{
  "HugeContract": {
    "runtime_size": 194,
    "init_size": 49344,
    "runtime_margin": 24382,
    "init_margin": -192
  }
} 
"#]]
            .is_json(),
        );
});

// tests build output is as expected
forgetest_init!(exact_build_output, |prj, cmd| {
    cmd.args(["build", "--force"]).assert_success().stdout_eq(str![[r#"
[COMPILING_FILES] with [SOLC_VERSION]
[SOLC_VERSION] [ELAPSED]
Compiler run successful!

"#]]);
});

// tests build output is as expected
forgetest_init!(build_sizes_no_forge_std, |prj, cmd| {
    cmd.args(["build", "--sizes"]).assert_success().stdout_eq(str![
        r#"
...
| Contract | Runtime Size (B) | Initcode Size (B) | Runtime Margin (B) | Initcode Margin (B) |
|----------|------------------|-------------------|--------------------|---------------------|
| Counter  |              236 |               263 |             24,340 |              48,889 |
...
"#
    ]);

    cmd.forge_fuse().args(["build", "--sizes", "--json"]).assert_success().stdout_eq(
        str![[r#"
{
  "Counter": {
    "runtime_size": 247,
    "init_size": 277,
    "runtime_margin": 24329,
    "init_margin": 48875
  }
} 
"#]]
        .is_json(),
    );
});

// tests that skip key in config can be used to skip non-compilable contract
forgetest_init!(test_can_skip_contract, |prj, cmd| {
    prj.add_source(
        "InvalidContract",
        r"
contract InvalidContract {
    some_invalid_syntax
}
",
    )
    .unwrap();

    prj.add_source(
        "ValidContract",
        r"
contract ValidContract {}
",
    )
    .unwrap();

    let config =
        Config { skip: vec![Glob::new("src/InvalidContract.sol").unwrap()], ..Default::default() };
    prj.write_config(config);

    cmd.args(["build"]).assert_success();
});

// tests build scenarios varying zk-detect-missing-libraries flag
// TOML config file could be used to set the flag
// also the flag should be set via command line argument
// In any of these cases, it should not build (if the flag is set)
// it should build (if the flag is not set in any of the above cases)
// case 1: [BUILD] flag in both toml config file and command line argument
forgetest_init!(test_zk_build_missing_libraries_config_and_flag, |prj, cmd| {
    let zk = ZkSyncConfig { detect_missing_libraries: true, ..Default::default() };
    prj.write_config(Config { zksync: zk, ..Default::default() });
    cmd.args(["build", "--zksync", "--zk-detect-missing-libraries"])
        .assert_success()
        // .stderr_eq(str![r#"Ignoring the `detect_missing_libraries` flag; it should not be used in
        // toml config file, but as an argument in the build command"#])
        .stdout_eq(str![[r#"
...
Compiler run successful with warnings:
...
"#]]);
    // .stderr_eq(str![r#"Ignoring the `detect_missing_libraries` flag; it should not be used in
    // toml config file, but as an argument in the build command"#]);
});

// scenario 2: [BUILD] flag set via command line argument only (NO WARNINGS; THE RIGHT WAY)
forgetest_init!(test_zk_build_missing_libraries_only_flag, |prj, cmd| {
    cmd.args(["build", "--zksync", "--zk-detect-missing-libraries"])
        .assert_failure()
        .stderr_eq(str![r#"Ignoring the `detect_missing_libraries` flag; it should not be used in toml config file, but as an argument in the build command"#]);
});

// scenario 3: [BUILD] flag set in toml config file only
forgetest_init!(test_zk_build_missing_libraries_only_config, |prj, cmd| {
    let zk = ZkSyncConfig { detect_missing_libraries: true, ..Default::default() };
    prj.write_config(Config { zksync: zk, ..Default::default() });
    cmd.args(["build", "--zksync"])
        .assert_failure()
        .stderr_eq(str![r#"Ignoring the `detect_missing_libraries` flag; it should not be used in toml config file, but as an argument in the build command"#]);
});

// scenario 4: [BUILD] flag not set in either toml config file or command line argument
forgetest_init!(test_zk_build_missing_libraries_none, |prj, cmd| {
    cmd.args(["build", "--zksync"]).assert_success();
});

// scenario 5: [TEST] flag not set in either toml config file or command line argument
forgetest_init!(test_zk_script_missing_libraries_config_and_flag, |prj, cmd| {
    let zk = ZkSyncConfig { detect_missing_libraries: true, ..Default::default() };
    prj.write_config(Config { zksync: zk, ..Default::default() });
    cmd.env("RUST_LOG", "warn");

    //     cmd.args(["test", "--zksync", "--zk-detect-missing-libraries-deprecated"])
    //         .assert_failure()
    //         .stderr_eq(str![[r#"
    // ...
    // Ignoring the `detect_missing_libraries` flag; it should not be used in toml config file, but
    // as an argument in the build command ...
    // "#]]);

    let output = cmd
        .args(["test", "--zksync", "--zk-detect-missing-libraries-deprecated"])
        .assert_failure()
        .get_output()
        .stdout_lossy();
    assert!(output.contains("Ignoring the `detect_missing_libraries` flag; it should not be used in toml config file, but as an argument in the build command"), "{}", output);

    // println!(
    //     "{}",
    //     cmd.args(["test", "--zksync", "--zk-detect-missing-libraries"])
    //         .assert_success()
    //         .get_output()
    //         .stdout_lossy()
    // );

    // .stderr_eq(str![r#"Ignoring the `detect_missing_libraries` flag; it should not be used in
    // toml config file, but as an argument in the build command"#])
    //         .stdout_eq(str![[r#"
    // ...
    // Compiler run successful with warnings:
    // ...
    // "#]]);
    // .stderr_eq(str![r#"Ignoring the `detect_missing_libraries` flag; it should not be used in
    // toml config file, but as an argument in the build command"#]);
});

// scenario 6: [TEST] flag not set in either toml config file or command line argument
forgetest_init!(test_zk_script_missing_libraries_config_and_flag, |prj, cmd| {
    let zk = ZkSyncConfig { detect_missing_libraries: true, ..Default::default() };
    prj.write_config(Config { zksync: zk, ..Default::default() });
    cmd.env("RUST_LOG", "warn");

    //     cmd.args(["test", "--zksync", "--zk-detect-missing-libraries-deprecated"])
    //         .assert_failure()
    //         .stderr_eq(str![[r#"
    // ...
    // Ignoring the `detect_missing_libraries` flag; it should not be used in toml config file, but
    // as an argument in the build command ...
    // "#]]);

    let output = cmd
        .args(["test", "--zksync", "--zk-detect-missing-libraries-deprecated"])
        .assert_failure()
        .get_output()
        .stdout_lossy();
    assert!(output.contains("Ignoring the `detect_missing_libraries` flag; it should not be used in toml config file, but as an argument in the build command"), "{}", output);

    // println!(
    //     "{}",
    //     cmd.args(["test", "--zksync", "--zk-detect-missing-libraries"])
    //         .assert_success()
    //         .get_output()
    //         .stdout_lossy()
    // );

    // .stderr_eq(str![r#"Ignoring the `detect_missing_libraries` flag; it should not be used in
    // toml config file, but as an argument in the build command"#])
    //         .stdout_eq(str![[r#"
    // ...
    // Compiler run successful with warnings:
    // ...
    // "#]]);
    // .stderr_eq(str![r#"Ignoring the `detect_missing_libraries` flag; it should not be used in
    // toml config file, but as an argument in the build command"#]);
});
