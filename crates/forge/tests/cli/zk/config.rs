//! Contains tests for checking configuration for zksync

use std::fs;

use foundry_test_utils::util::OutputExt;

// test to check that the config is not skipped when using the wrong settings in the foundry.toml
// and emits the correct warnings
forgetest!(test_zk_foundry_toml_config_error_does_not_skip_correct_settings, |prj, cmd| {
    let faulty_toml = r"
    [profile.default]
    src = 'src'
    out = 'out'
    libs = ['lib']
    solc_version = '0.8.20'

    [profile.default.zksync]
    suppressed_errors = ['invalid-error', 'sendtransfer']
    suppressed_warnings = ['invalid-warning']
    zksolc='1.5.10'";

    prj.add_source("Greeter.sol", include_str!("../../../../../testdata/zk/Greeter.sol")).unwrap();

    fs::write(prj.root().join("foundry.toml"), faulty_toml).unwrap();

    let output = cmd
        .forge_fuse()
        .arg("build")
        .arg("--zksync")
        .arg("--build-info")
        .assert_success()
        .get_output()
        .stdout_lossy();

    assert!(output.contains("Invalid suppressed error type: invalid-error"));
    assert!(output.contains("Invalid suppressed warning type: invalid-warning"));

    // read build info to assert that the version of zksolc was not skipped
    let build_info = fs::read_to_string(
        prj.root().join("zkout").join("build-info").join(
            fs::read_dir(prj.root().join("zkout").join("build-info"))
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .file_name(),
        ),
    )
    .unwrap();
    assert!(build_info.contains("1.5.10"));
});
