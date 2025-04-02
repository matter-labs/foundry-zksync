use clap::{CommandFactory, Parser};
use clap_complete::generate;
use eyre::Result;
use foundry_cli::{handler, utils};
use foundry_common::{shell, POSTHOG_API_KEY, TELEMETRY_CONFIG_NAME};
use foundry_evm::inspectors::cheatcodes::{set_execution_context, ForgeContext};
use zksync_telemetry::{get_telemetry, init_telemetry, TelemetryProps};

mod cmd;
use cmd::{cache::CacheSubcommands, generate::GenerateSubcommands, watch};

mod opts;
use opts::{Forge, ForgeSubcommand};

#[macro_use]
extern crate foundry_common;

#[macro_use]
extern crate tracing;

#[cfg(all(feature = "jemalloc", unix))]
#[global_allocator]
static ALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

fn main() {
    let _ = utils::block_on(init_telemetry(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        TELEMETRY_CONFIG_NAME,
        Some(POSTHOG_API_KEY.into()),
        None,
        None,
    ));
    if let Err(err) = run() {
        let _ = foundry_common::sh_err!("{err:?}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    handler::install();
    utils::load_dotenv();
    utils::subscriber();
    utils::enable_paint();

    let args = Forge::parse();
    args.global.init()?;
    init_execution_context(&args.cmd);

    let command_name: &str;
    let mut subcommand_name: Option<&str> = None;
    let result = match args.cmd {
        ForgeSubcommand::Test(cmd) => {
            command_name = "test";
            if cmd.is_watch() {
                utils::block_on(watch::watch_test(cmd))
            } else {
                let silent = cmd.junit || shell::is_json();
                let outcome = utils::block_on(cmd.run())?;
                outcome.ensure_ok(silent)
            }
        }
        ForgeSubcommand::Script(cmd) => {
            command_name = "script";
            utils::block_on(cmd.run_script())
        }
        ForgeSubcommand::Coverage(cmd) => {
            command_name = "coverage";
            if cmd.is_watch() {
                utils::block_on(watch::watch_coverage(cmd))
            } else {
                utils::block_on(cmd.run())
            }
        }
        ForgeSubcommand::Bind(cmd) => {
            command_name = "bind";
            cmd.run()
        }
        ForgeSubcommand::Build(cmd) => {
            command_name = "build";
            if cmd.is_watch() {
                utils::block_on(watch::watch_build(cmd))
            } else {
                cmd.run().map(drop)
            }
        }
        ForgeSubcommand::VerifyContract(args) => {
            command_name = "verify-contract";
            utils::block_on(args.run())
        }
        ForgeSubcommand::VerifyCheck(args) => {
            command_name = "verify-check";
            utils::block_on(args.run())
        }
        ForgeSubcommand::VerifyBytecode(cmd) => {
            command_name = "verify-bytecode";
            utils::block_on(cmd.run())
        }
        ForgeSubcommand::Clone(cmd) => {
            command_name = "clone";
            utils::block_on(cmd.run())
        }
        ForgeSubcommand::Cache(cmd) => {
            command_name = "cache";
            match cmd.sub {
                CacheSubcommands::Clean(cmd) => {
                    subcommand_name = Some("clean");
                    cmd.run()
                }
                CacheSubcommands::Ls(cmd) => {
                    subcommand_name = Some("ls");
                    cmd.run()
                }
            }
        }
        ForgeSubcommand::Create(cmd) => {
            command_name = "create";
            utils::block_on(cmd.run())
        }
        ForgeSubcommand::Update(cmd) => {
            command_name = "update";
            cmd.run()
        }
        ForgeSubcommand::Install(cmd) => {
            command_name = "install";
            cmd.run()
        }
        ForgeSubcommand::Remove(cmd) => {
            command_name = "remove";
            cmd.run()
        }
        ForgeSubcommand::Remappings(cmd) => {
            command_name = "remappings";
            cmd.run()
        }
        ForgeSubcommand::Init(cmd) => {
            command_name = "init";
            cmd.run()
        }
        ForgeSubcommand::Completions { shell } => {
            command_name = "completions";
            generate(shell, &mut Forge::command(), "forge", &mut std::io::stdout());
            Ok(())
        }
        ForgeSubcommand::GenerateFigSpec => {
            command_name = "generate-fig-spec";
            clap_complete::generate(
                clap_complete_fig::Fig,
                &mut Forge::command(),
                "forge",
                &mut std::io::stdout(),
            );
            Ok(())
        }
        ForgeSubcommand::Clean { root } => {
            command_name = "clean";
            let config = utils::load_config_with_root(root.as_deref())?;
            let project = config.project()?;
            let zk_project =
                foundry_config::zksync::config_create_project(&config, config.cache, false)?;
            config.cleanup(&project)?;
            config.cleanup(&zk_project)?;
            Ok(())
        }
        ForgeSubcommand::Snapshot(cmd) => {
            command_name = "snapshot";
            if cmd.is_watch() {
                utils::block_on(watch::watch_gas_snapshot(cmd))
            } else {
                utils::block_on(cmd.run())
            }
        }
        ForgeSubcommand::Fmt(cmd) => {
            command_name = "fmt";
            if cmd.is_watch() {
                utils::block_on(watch::watch_fmt(cmd))
            } else {
                cmd.run()
            }
        }
        ForgeSubcommand::Config(cmd) => {
            command_name = "config";
            cmd.run()
        }
        ForgeSubcommand::Flatten(cmd) => {
            command_name = "flatten";
            cmd.run()
        }
        ForgeSubcommand::Inspect(cmd) => {
            command_name = "inspect";
            cmd.run()
        }
        ForgeSubcommand::Tree(cmd) => {
            command_name = "tree";
            cmd.run()
        }
        ForgeSubcommand::Geiger(cmd) => {
            command_name = "geiger";
            let n = cmd.run()?;
            if n > 0 {
                std::process::exit(n as i32);
            }
            Ok(())
        }
        ForgeSubcommand::Doc(cmd) => {
            command_name = "doc";
            if cmd.is_watch() {
                utils::block_on(watch::watch_doc(cmd))
            } else {
                utils::block_on(cmd.run())?;
                Ok(())
            }
        }
        ForgeSubcommand::Selectors { command } => {
            command_name = "selectors";
            utils::block_on(command.run())
        }
        ForgeSubcommand::Generate(cmd) => {
            command_name = "generate";
            match cmd.sub {
                GenerateSubcommands::Test(cmd) => {
                    subcommand_name = Some("test");
                    cmd.run()
                }
            }
        }
        ForgeSubcommand::Compiler(cmd) => {
            command_name = "compiler";
            cmd.run()
        }
        ForgeSubcommand::Soldeer(cmd) => {
            command_name = "soldeer";
            utils::block_on(cmd.run())
        }
        ForgeSubcommand::Eip712(cmd) => {
            command_name = "eip712";
            cmd.run()
        }
        ForgeSubcommand::BindJson(cmd) => {
            command_name = "bind-json";
            cmd.run()
        }
    };

    let telemetry = get_telemetry().expect("telemetry is not initialized");
    let telemetry_cmd_params = TelemetryProps::new()
        .insert("command", Some(command_name))
        .insert("subcommand", subcommand_name)
        .take();
    let _ = utils::block_on(telemetry.track_event(
        "forge",
        TelemetryProps::new().insert("params", Some(telemetry_cmd_params)).take(),
    ));

    result
}

/// Set the program execution context based on `forge` subcommand used.
/// The execution context can be set only once per program, and it can be checked by using
/// cheatcodes.
fn init_execution_context(subcommand: &ForgeSubcommand) {
    let context = match subcommand {
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
}
