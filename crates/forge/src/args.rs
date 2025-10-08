use crate::{
    cmd::{cache::CacheSubcommands, generate::GenerateSubcommands, watch},
    opts::{Forge, ForgeSubcommand},
};
use clap::{CommandFactory, Parser};
use clap_complete::generate;
use eyre::Result;
use foundry_cli::utils;
use foundry_common::shell;
use foundry_evm::inspectors::cheatcodes::{ForgeContext, set_execution_context};
use zksync_telemetry::{TelemetryProps, get_telemetry};

/// Run the `forge` command line interface.
pub fn run() -> Result<()> {
    setup()?;

    let args = Forge::parse();
    args.global.init()?;

    run_command(args)
}

/// Setup the global logger and other utilities.
pub fn setup() -> Result<()> {
    utils::common_setup();
    utils::subscriber();

    Ok(())
}

/// Run the subcommand.
pub fn run_command(args: Forge) -> Result<()> {
    let telemetry = get_telemetry().expect("telemetry is not initialized");
    let telemetry_props = args.cmd.get_telemetry_props();
    // Set the execution context based on the subcommand.
    let context = match &args.cmd {
        ForgeSubcommand::Test(_) => ForgeContext::Test,
        ForgeSubcommand::Coverage(_) => ForgeContext::Coverage,
        ForgeSubcommand::Snapshot(_) => ForgeContext::Snapshot,
        ForgeSubcommand::Script(cmd) => {
            if cmd.broadcast {
                ForgeContext::ScriptBroadcast
            } else if cmd.resume {
                ForgeContext::ScriptResume
            } else {
                ForgeContext::ScriptDryRun
            }
        }
        _ => ForgeContext::Unknown,
    };
    set_execution_context(context);

    let global = &args.global;

    // Run the subcommand.
    let result = match args.cmd {
        ForgeSubcommand::Test(cmd) => {
            if cmd.is_watch() {
                global.block_on(watch::watch_test(cmd))
            } else {
                let silent = cmd.junit || shell::is_json();
                let outcome = global.block_on(cmd.run())?;
                outcome.ensure_ok(silent)
            }
        }
        ForgeSubcommand::Script(cmd) => global.block_on(cmd.run_script()),
        ForgeSubcommand::Coverage(cmd) => {
            if cmd.is_watch() {
                global.block_on(watch::watch_coverage(cmd))
            } else {
                global.block_on(cmd.run())
            }
        }
        ForgeSubcommand::Bind(cmd) => cmd.run(),
        ForgeSubcommand::Build(cmd) => {
            if cmd.is_watch() {
                global.block_on(watch::watch_build(cmd))
            } else {
                global.block_on(cmd.run()).map(drop)
            }
        }
        ForgeSubcommand::VerifyContract(args) => global.block_on(args.run()),
        ForgeSubcommand::VerifyCheck(args) => global.block_on(args.run()),
        ForgeSubcommand::VerifyBytecode(cmd) => global.block_on(cmd.run()),
        ForgeSubcommand::Clone(cmd) => global.block_on(cmd.run()),
        ForgeSubcommand::Cache(cmd) => match cmd.sub {
            CacheSubcommands::Clean(cmd) => cmd.run(),
            CacheSubcommands::Ls(cmd) => cmd.run(),
        },
        ForgeSubcommand::Create(cmd) => global.block_on(cmd.run()),
        ForgeSubcommand::Update(cmd) => cmd.run(),
        ForgeSubcommand::Install(cmd) => global.block_on(cmd.run()),
        ForgeSubcommand::Remove(cmd) => cmd.run(),
        ForgeSubcommand::Remappings(cmd) => cmd.run(),
        ForgeSubcommand::Init(cmd) => global.block_on(cmd.run()),
        ForgeSubcommand::Completions { shell } => {
            generate(shell, &mut Forge::command(), "forge", &mut std::io::stdout());
            Ok(())
        }
        ForgeSubcommand::Clean { root } => {
            let config = utils::load_config_with_root(root.as_deref())?;
            let project = config.project()?;
            let zk_project =
                foundry_config::zksync::config_create_project(&config, config.cache, false)?;
            config.cleanup(&project)?;
            config.cleanup(&zk_project)?;
            Ok(())
        }
        ForgeSubcommand::Snapshot(cmd) => {
            if cmd.is_watch() {
                global.block_on(watch::watch_gas_snapshot(cmd))
            } else {
                global.block_on(cmd.run())
            }
        }
        ForgeSubcommand::Fmt(cmd) => {
            if cmd.is_watch() {
                global.block_on(watch::watch_fmt(cmd))
            } else {
                cmd.run()
            }
        }
        ForgeSubcommand::Config(cmd) => cmd.run(),
        ForgeSubcommand::Flatten(cmd) => cmd.run(),
        ForgeSubcommand::Inspect(cmd) => cmd.run(),
        ForgeSubcommand::Tree(cmd) => cmd.run(),
        ForgeSubcommand::Geiger(cmd) => cmd.run(),
        ForgeSubcommand::Doc(cmd) => {
            if cmd.is_watch() {
                global.block_on(watch::watch_doc(cmd))
            } else {
                global.block_on(cmd.run())?;
                Ok(())
            }
        }
        ForgeSubcommand::Selectors { command } => global.block_on(command.run()),
        ForgeSubcommand::Generate(cmd) => match cmd.sub {
            GenerateSubcommands::Test(cmd) => cmd.run(),
        },
        ForgeSubcommand::Compiler(cmd) => cmd.run(),
        ForgeSubcommand::Soldeer(cmd) => global.block_on(cmd.run()),
        ForgeSubcommand::Eip712(cmd) => cmd.run(),
        ForgeSubcommand::BindJson(cmd) => cmd.run(),
        ForgeSubcommand::Lint(cmd) => cmd.run(),
    };

    let _ = args.global.tokio_runtime().block_on(
        telemetry.track_event(
            "forge",
            TelemetryProps::new()
                .insert("params", Some(telemetry_props))
                .insert("result", Some(if result.is_ok() { "success" } else { "failure" }))
                .take(),
        ),
    );

    result
}
