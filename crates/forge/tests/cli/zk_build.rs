use foundry_common::fs;
use foundry_test_utils::util::OutputExt;
use regex::Regex;

// tests build output is as expected in zksync mode
forgetest_init!(test_zk_build_sizes, |prj, cmd| {
    cmd.args(["build", "--sizes", "--zksync", "--evm-version", "shanghai"]);
    let stdout = cmd.assert_success().get_output().stdout_lossy();
    let pattern =
        Regex::new(r"\|\s*Counter\s*\|\s*800\s*\|\s*800\s*\|\s*450,199\s*\|\s*450,199\s*\|")
            .unwrap();

    assert!(pattern.is_match(&stdout), "Unexpected size output:\n{stdout}");
});

// tests build output is as expected in zksync mode
forgetest_init!(test_zk_cache_ok, |prj, cmd| {
    let zk_toml = r#"[profile.default]
src = 'src'
out = 'out'
libs = ['lib']
solc = '0.8.26'

[profile.default.zksync]
zksolc = '1.5.6'
"#;

    fs::write(prj.root().join("foundry.toml"), zk_toml).unwrap();

    cmd.args(["build", "--zksync"]);
    let stdout_1 = cmd.assert_success().get_output().stdout_lossy();
    let pattern_1 = Regex::new(r"Compiler run successful").unwrap();

    let stdout_2 = cmd.assert_success().get_output().stdout_lossy();
    let pattern_2 = Regex::new(r"No files changed, compilation skipped").unwrap();

    assert!(pattern_1.is_match(&stdout_1));
    assert!(pattern_2.is_match(&stdout_2));
});

// tests build output is as expected in zksync mode
forgetest_init!(test_zk_cache_invalid_on_version_changed, |prj, cmd| {
    let template_toml = r#"[profile.default]
src = 'src'
out = 'out'
libs = ['lib']
solc = '0.8.26'

[profile.default.zksync]
"#;

    let toml_156 = format!(
        r#"{}
zksolc = '1.5.6'
"#,
        template_toml
    );

    let toml_157 = format!(
        r#"{}
zksolc = '1.5.7'
"#,
        template_toml
    );

    fs::write(prj.root().join("foundry.toml"), toml_156).unwrap();

    cmd.args(["build", "--zksync"]);
    let stdout_1 = cmd.assert_success().get_output().stdout_lossy();
    let pattern_1 = Regex::new(r"Compiler run successful").unwrap();

    fs::remove_file(prj.root().join("foundry.toml")).unwrap();
    fs::write(prj.root().join("foundry.toml"), toml_157).unwrap();

    let stdout_2 = cmd.assert_success().get_output().stdout_lossy();
    let pattern_2 = Regex::new(r"Compiler run successful!").unwrap(); // if we see this, means the cache was invalidated

    assert!(pattern_1.is_match(&stdout_1));
    assert!(pattern_2.is_match(&stdout_2));
});
