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
