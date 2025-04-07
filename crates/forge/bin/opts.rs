use crate::cmd::{
    bind::BindArgs,
    bind_json,
    build::BuildArgs,
    cache::{CacheArgs, CacheSubcommands},
    clone::CloneArgs,
    compiler::CompilerArgs,
    config, coverage,
    create::CreateArgs,
    doc::DocArgs,
    eip712, flatten,
    fmt::FmtArgs,
    geiger, generate,
    generate::GenerateSubcommands,
    init::InitArgs,
    inspect,
    install::InstallArgs,
    remappings::RemappingArgs,
    remove::RemoveArgs,
    selectors::SelectorsSubcommands,
    snapshot, soldeer, test, tree, update,
};
use clap::{Parser, Subcommand, ValueHint};
use forge_script::ScriptArgs;
use forge_verify::{VerifyArgs, VerifyBytecodeArgs, VerifyCheckArgs};
use foundry_cli::opts::GlobalArgs;
use foundry_common::version::{LONG_VERSION, SHORT_VERSION};
use std::path::PathBuf;
use zksync_telemetry::TelemetryProps;

/// Build, test, fuzz, debug and deploy Solidity contracts.
#[derive(Parser)]
#[command(
    name = "forge",
    version = SHORT_VERSION,
    long_version = LONG_VERSION,
    after_help = "Find more information in the book: http://book.getfoundry.sh/reference/forge/forge.html",
    next_display_order = None,
)]
pub struct Forge {
    /// Include the global arguments.
    #[command(flatten)]
    pub global: GlobalArgs,

    #[command(subcommand)]
    pub cmd: ForgeSubcommand,
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum ForgeSubcommand {
    /// Run the project's tests.
    #[command(visible_alias = "t")]
    Test(test::TestArgs),

    /// Run a smart contract as a script, building transactions that can be sent onchain.
    Script(ScriptArgs),

    /// Generate coverage reports.
    Coverage(coverage::CoverageArgs),

    /// Generate Rust bindings for smart contracts.
    #[command(alias = "bi")]
    Bind(BindArgs),

    /// Build the project's smart contracts.
    #[command(visible_aliases = ["b", "compile"])]
    Build(BuildArgs),

    /// Clone a contract from Etherscan.
    Clone(CloneArgs),

    /// Update one or multiple dependencies.
    ///
    /// If no arguments are provided, then all dependencies are updated.
    #[command(visible_alias = "u")]
    Update(update::UpdateArgs),

    /// Install one or multiple dependencies.
    ///
    /// If no arguments are provided, then existing dependencies will be installed.
    #[command(visible_alias = "i")]
    Install(InstallArgs),

    /// Remove one or multiple dependencies.
    #[command(visible_alias = "rm")]
    Remove(RemoveArgs),

    /// Get the automatically inferred remappings for the project.
    #[command(visible_alias = "re")]
    Remappings(RemappingArgs),

    /// Verify smart contracts on Etherscan.
    #[command(visible_alias = "v")]
    VerifyContract(VerifyArgs),

    /// Check verification status on Etherscan.
    #[command(visible_alias = "vc")]
    VerifyCheck(VerifyCheckArgs),

    /// Verify the deployed bytecode against its source on Etherscan.
    #[command(visible_alias = "vb")]
    VerifyBytecode(VerifyBytecodeArgs),

    /// Deploy a smart contract.
    #[command(visible_alias = "c")]
    Create(CreateArgs),

    /// Create a new Forge project.
    Init(InitArgs),

    /// Generate shell completions script.
    #[command(visible_alias = "com")]
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },

    /// Generate Fig autocompletion spec.
    #[command(visible_alias = "fig")]
    GenerateFigSpec,

    /// Remove the build artifacts and cache directories.
    #[command(visible_alias = "cl")]
    Clean {
        /// The project's root path.
        ///
        /// By default root of the Git repository, if in one,
        /// or the current working directory.
        #[arg(long, value_hint = ValueHint::DirPath, value_name = "PATH")]
        root: Option<PathBuf>,
    },

    /// Manage the Foundry cache.
    Cache(CacheArgs),

    /// Create a gas snapshot of each test's gas usage.
    #[command(visible_alias = "s")]
    Snapshot(snapshot::GasSnapshotArgs),

    /// Display the current config.
    #[command(visible_alias = "co")]
    Config(config::ConfigArgs),

    /// Flatten a source file and all of its imports into one file.
    #[command(visible_alias = "f")]
    Flatten(flatten::FlattenArgs),

    /// Format Solidity source files.
    Fmt(FmtArgs),

    /// Get specialized information about a smart contract.
    #[command(visible_alias = "in")]
    Inspect(inspect::InspectArgs),

    /// Display a tree visualization of the project's dependency graph.
    #[command(visible_alias = "tr")]
    Tree(tree::TreeArgs),

    /// Detects usage of unsafe cheat codes in a project and its dependencies.
    Geiger(geiger::GeigerArgs),

    /// Generate documentation for the project.
    Doc(DocArgs),

    /// Function selector utilities.
    #[command(visible_alias = "se")]
    Selectors {
        #[command(subcommand)]
        command: SelectorsSubcommands,
    },

    /// Generate scaffold files.
    Generate(generate::GenerateArgs),

    /// Compiler utilities.
    Compiler(CompilerArgs),

    /// Soldeer dependency manager.
    Soldeer(soldeer::SoldeerArgs),

    /// Generate EIP-712 struct encodings for structs from a given file.
    Eip712(eip712::Eip712Args),

    /// Generate bindings for serialization/deserialization of project structs via JSON cheatcodes.
    BindJson(bind_json::BindJsonArgs),
}

impl ForgeSubcommand {
    pub fn get_telemetry_props(&self) -> TelemetryProps {
        let (command_name, subcommand_name) = match self {
            ForgeSubcommand::Test(_) => ("test", None),
            ForgeSubcommand::Script(_) => ("script", None),
            ForgeSubcommand::Coverage(_) => ("coverage", None),
            ForgeSubcommand::Bind(_) => ("bind", None),
            ForgeSubcommand::Build(_) => ("build", None),
            ForgeSubcommand::VerifyContract(_) => ("verify-contract", None),
            ForgeSubcommand::VerifyCheck(_) => ("verify-check", None),
            ForgeSubcommand::VerifyBytecode(_) => ("verify-bytecode", None),
            ForgeSubcommand::Clone(_) => ("clone", None),
            ForgeSubcommand::Cache(cmd) => match cmd.sub {
                CacheSubcommands::Clean(_) => ("cache", Some("clean")),
                CacheSubcommands::Ls(_) => ("cache", Some("ls")),
            },
            ForgeSubcommand::Create(_) => ("create", None),
            ForgeSubcommand::Update(_) => ("update", None),
            ForgeSubcommand::Install(_) => ("install", None),
            ForgeSubcommand::Remove(_) => ("remove", None),
            ForgeSubcommand::Remappings(_) => ("remappings", None),
            ForgeSubcommand::Init(_) => ("init", None),
            ForgeSubcommand::Completions { shell: _ } => ("completions", None),
            ForgeSubcommand::GenerateFigSpec => ("generate-fig-spec", None),
            ForgeSubcommand::Clean { root: _ } => ("clean", None),
            ForgeSubcommand::Snapshot(_) => ("snapshot", None),
            ForgeSubcommand::Fmt(_) => ("fmt", None),
            ForgeSubcommand::Config(_) => ("config", None),
            ForgeSubcommand::Flatten(_) => ("flatten", None),
            ForgeSubcommand::Inspect(_) => ("inspect", None),
            ForgeSubcommand::Tree(_) => ("tree", None),
            ForgeSubcommand::Geiger(_) => ("geiger", None),
            ForgeSubcommand::Doc(_) => ("doc", None),
            ForgeSubcommand::Selectors { command: _ } => ("selectors", None),
            ForgeSubcommand::Generate(cmd) => match cmd.sub {
                GenerateSubcommands::Test(_) => ("generate", Some("test")),
            },
            ForgeSubcommand::Compiler(_) => ("compiler", None),
            ForgeSubcommand::Soldeer(_) => ("soldeer", None),
            ForgeSubcommand::Eip712(_) => ("eip712", None),
            ForgeSubcommand::BindJson(_) => ("bind-json", None),
        };
        TelemetryProps::new()
            .insert("command", Some(command_name))
            .insert("subcommand", subcommand_name)
            .take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn verify_cli() {
        Forge::command().debug_assert();
    }
}
