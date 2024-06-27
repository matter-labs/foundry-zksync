//! # foundry-zksync
//!
//! Main Foundry ZKSync implementation.
#![warn(missing_docs, unused_crate_dependencies)]

/// ZKSolc specific logic.
mod zksolc;

pub use zksolc::*;

pub mod libraries;

use foundry_compilers::{
    error::SolcError,
    multi::MultiCompilerLanguage,
    solc::{SolcCompiler, SolcLanguage},
    zksync::compile::output::ProjectCompileOutput,
    ConfigurableArtifacts, Project, ProjectBuilder, ProjectPathsConfig,
};

/// Compile zksync artifacts
pub fn compile_project(project: &Project) -> std::result::Result<ProjectCompileOutput, SolcError> {
    let mut zk_project = ProjectBuilder::<SolcCompiler, ConfigurableArtifacts>::default()
        .locked_versions(
            project
                .locked_versions
                .clone()
                .into_iter()
                .filter_map(|(language, version)| match language {
                    MultiCompilerLanguage::Solc(solc_lang) => Some((solc_lang, version)),
                    _ => None,
                })
                .collect(),
        )
        .paths(ProjectPathsConfig {
            _l: std::marker::PhantomData::<SolcLanguage>::default(),
            root: project.paths.root.clone(),
            cache: project.paths.cache.clone(),
            artifacts: project.paths.artifacts.clone(),
            build_infos: project.paths.build_infos.clone(),
            sources: project.paths.sources.clone(),
            tests: project.paths.tests.clone(),
            scripts: project.paths.scripts.clone(),
            libraries: project.paths.libraries.clone(),
            remappings: project.paths.remappings.clone(),
            include_paths: project.paths.include_paths.clone(),
            allowed_paths: project.paths.allowed_paths.clone(),
            zksync_artifacts: project.paths.zksync_artifacts.clone(),
            zksync_cache: project.paths.zksync_cache.clone(),
        })
        .settings(project.settings.solc.clone())
        .set_cached(project.cached)
        .set_build_info(project.build_info)
        .set_no_artifacts(project.no_artifacts)
        .artifacts(project.artifacts)
        .ignore_error_codes(project.ignored_error_codes.clone())
        .ignore_paths(project.ignored_file_paths.clone())
        .set_compiler_severity_filter(project.compiler_severity_filter)
        .single_solc_jobs()
        .set_offline(project.offline)
        .set_slashed_paths(project.slash_paths)
        .build(project.compiler.solc.clone())?;
    zk_project.sparse_output = project.sparse_output.clone();
    zk_project.zksync_zksolc = project.zksync_zksolc.clone();
    zk_project.zksync_zksolc_config = project.zksync_zksolc_config.clone();
    zk_project.zksync_artifacts = project.zksync_artifacts.clone();
    zk_project.zksync_avoid_contracts = project.zksync_avoid_contracts.clone();

    foundry_compilers::zksync::project_compile(&zk_project)
}
