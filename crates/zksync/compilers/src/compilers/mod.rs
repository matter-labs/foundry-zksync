//! `foundry-compilers` core trait implementations and overrides for ZK Sync
pub mod artifact_output;
pub mod zksolc;

use std::path::{Path, PathBuf};

use alloy_primitives::map::HashSet;
use artifact_output::zk::ZkArtifactOutput;
use foundry_compilers::{
    error::{Result, SolcError},
    resolver::parse::SolData,
    Graph, Project,
};
use zksolc::{input::StandardJsonCompilerInput, ZkSolcCompiler, ZkSolcSettings};

/// zksolc specific standard_json_input to be used in verification
// https://github.com/foundry-rs/compilers/blob/ff2a8d68a0d85d8f40c545a7a948e84d1bc2488e/crates/compilers/src/lib.rs#L165
// TODO: foundry_compilers only implements this for compilers that impment Into<SolcSettings>.
// Maybe this works for us or maybe we can submit required changes upstream
pub fn project_standard_json_input(
    project: &Project<ZkSolcCompiler, ZkArtifactOutput>,
    target: &Path,
) -> Result<StandardJsonCompilerInput> {
    tracing::debug!(?target, "standard_json_input for zksync");
    let graph = Graph::<SolData>::resolve(&project.paths)?;
    let target_index = graph
        .files()
        .get(target)
        .ok_or_else(|| SolcError::msg(format!("cannot resolve file at {:?}", target.display())))?;

    let mut sources = Vec::new();
    let mut unique_paths = HashSet::new();
    let (path, source) = graph.node(*target_index).unpack();
    unique_paths.insert(path.clone());
    sources.push((path, source));
    sources.extend(
        graph
            .all_imported_nodes(*target_index)
            .map(|index| graph.node(index).unpack())
            .filter(|(p, _)| unique_paths.insert(p.to_path_buf())),
    );

    let root = project.root();
    let sources = sources
        .into_iter()
        .map(|(path, source)| (rebase_path(root, path), source.clone()))
        .collect();

    let mut zk_solc_settings: ZkSolcSettings = project.settings.clone();
    // strip the path to the project root from all remappings
    zk_solc_settings.settings.remappings = project
        .paths
        .remappings
        .clone()
        .into_iter()
        .map(|r| r.into_relative(project.root()).to_relative_remapping())
        .collect::<Vec<_>>();

    zk_solc_settings.settings.libraries.libs = zk_solc_settings
        .settings
        .libraries
        .libs
        .into_iter()
        .map(|(f, libs)| (f.strip_prefix(project.root()).unwrap_or(&f).to_path_buf(), libs))
        .collect();

    let input = StandardJsonCompilerInput::new(sources, zk_solc_settings.settings);

    Ok(input)
}

// Copied from compilers/lib private method
fn rebase_path(base: &Path, path: &Path) -> PathBuf {
    use path_slash::PathExt;

    let mut base_components = base.components();
    let mut path_components = path.components();

    let mut new_path = PathBuf::new();

    while let Some(path_component) = path_components.next() {
        let base_component = base_components.next();

        if Some(path_component) != base_component {
            if base_component.is_some() {
                new_path.extend(
                    std::iter::repeat(std::path::Component::ParentDir)
                        .take(base_components.count() + 1),
                );
            }

            new_path.push(path_component);
            new_path.extend(path_components);

            break;
        }
    }

    new_path.to_slash_lossy().into_owned().into()
}
