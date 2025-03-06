use crate::foundry_test_utils::util::OutputExt;
use foundry_config::Config;
use foundry_test_utils::forgetest;

forgetest!(test_zk_inspect, |prj, cmd| {
    prj.add_source(
        "Contracts.sol",
        r#"
//SPDX-license-identifier: MIT

pragma solidity ^0.8.20;

contract ContractOne {
    int public i;

    constructor() {
        i = 0;
    }

    function foo() public{
        while(i<5){
            i++;
        }
    }
}
    "#,
    )
    .unwrap();

    prj.write_config(Config {
        gas_reports: (vec!["*".to_string()]),
        gas_reports_ignore: (vec![]),
        ..Default::default()
    });

    let out_solc_bytecode = cmd
        .arg("inspect")
        .arg("ContractOne")
        .arg("bytecode")
        .assert_success()
        .get_output()
        .stdout_lossy();
    cmd.forge_fuse();

    let out_solc_bytecode = out_solc_bytecode.lines().last().expect("inspect returns output");

    let out_zk_bytecode = cmd
        .arg("inspect")
        .arg("ContractOne")
        .arg("bytecode")
        .arg("--zksync")
        .assert_success()
        .get_output()
        .stdout_lossy();
    cmd.forge_fuse();

    let out_zk_bytecode = out_zk_bytecode.lines().last().expect("inspect returns output");

    let out_deployedbytecode = cmd
        .arg("inspect")
        .arg("ContractOne")
        .arg("deployedbytecode")
        .arg("--zksync")
        .assert_success()
        .get_output()
        .stdout_lossy();
    cmd.forge_fuse();

    let out_deployedbytecode = out_deployedbytecode.lines().last().expect("inspect returns output");

    // The solc and zksolc bytecodes returned by inspect should be different
    assert_ne!(out_solc_bytecode, out_zk_bytecode);

    // The deployed bytecode in our case should be the same as the bytecode
    assert_eq!(out_zk_bytecode, out_deployedbytecode);

    // Throw an error when trying to inspect the assembly field
    cmd.arg("inspect")
        .arg("ContractOne")
        .arg("assembly")
        .arg("--zksync")
        .assert_failure()
        .stderr_eq("Error: ZKsync version of inspect does not support this field\n");
});
