use dirs;
use anyhow::{Result, Error};
use downloader::{Downloader, Download};
use serde::Serialize;
use std::{fmt, fs, os::{unix::prelude::PermissionsExt}, path::PathBuf, time::Duration};
use url::Url;

#[derive(Debug, Clone, Serialize)]
enum ZkSolcVersion {
    V000,
    V001,
}

impl ZkSolcVersion {
    fn get_version(&self) -> &str {
        match self {
            ZkSolcVersion::V000 => "v0.0.0",
            ZkSolcVersion::V001 => "v0.0.1",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
enum ZkSolcOS {
    Linux,
    Mac,
}

impl ZkSolcOS {
    fn get_compiler(&self) -> &str {
        match self {
            ZkSolcOS::Linux => "zksolc-linux-amd64-musl-",
            ZkSolcOS::Mac => "zksolc-macosx-arm64-",
        }
    }
}

fn get_operating_system() -> Result<ZkSolcOS> {
    match std::env::consts::OS {
        "linux" => Ok(ZkSolcOS::Linux),
        "macos" | "darwin" => Ok(ZkSolcOS::Mac),
        _ => Err(Error::msg("Unsupported opeating system")),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ZkSolcManagerOpts {
    pub version: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ZkSolcManagerBuilder {
    compilers_path: Option<PathBuf>,
    pub version: String,
    compiler: Option<String>,
    pub download_url: Url,
}

impl ZkSolcManagerBuilder {
    pub fn new(opts: ZkSolcManagerOpts) -> Self {
        Self { 
            compilers_path: None, 
            version: opts.version, 
            compiler: None, 
            download_url: Url::parse("https://github.com/matter-labs/zksolc-bin/raw/main/").unwrap(),
        }
    }

    fn get_compiler(self) -> Result<String> {
        if let Ok(zk_solc_os) = get_operating_system() {
            let compiler = zk_solc_os.get_compiler().to_string();
            Ok(compiler)
        } else {
            Err(Error::msg("Could not determine compiler"))
        }
    }

    pub fn build(self) -> Result<ZkSolcManager> {
        if let Some(mut home_path) = dirs::home_dir() {
            home_path.push(".zksync");
            let version = self.version.to_string();
            let download_url = self.download_url.to_owned();
            let compiler = self.get_compiler()?;
            let compilers_path = home_path.to_owned();

            return Ok(ZkSolcManager {
                compilers_path,
                version,
                compiler,
                download_url,
            });
        }
        Err(Error::msg("Could not build ZkSolcManager"))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ZkSolcManager {
    pub compilers_path: PathBuf,
    pub version: String,
    pub compiler: String,
    pub download_url: Url,
}

impl fmt::Display for ZkSolcManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, 
            "ZkSolcManager (
                compilers_path: {}, 
                version: {}, 
                compiler: {}, 
                download_url: {}, 
                compiler_name: {}, 
                full_download_url: {}
                exists: {}
            )", 
            self.compilers_path.display(), 
            self.version, 
            self.compiler, 
            self.download_url, 
            self.clone().get_full_compiler(), 
            self.clone().get_full_download_url(),
            self.clone().exists(),
        )
    }
}

impl ZkSolcManager {
    pub fn get_full_compiler(self) -> String {
        return format!("{}{}", self.compiler, self.version);
    }

    pub fn get_full_download_url(&self) -> String {
        return format!("{}{}", self.download_url, self.clone().get_full_compiler());
    }

    pub fn exists(&self) -> bool {
        let compiler_path = self.compilers_path.join(self.clone().get_full_compiler());
        if let Ok(metadata) = fs::metadata(compiler_path) {
            if metadata.is_file() && metadata.permissions().mode() & 0o755 != 0 {
                return true;
            }
        }
        false
    }

    pub async fn download(self) -> Result<()> {
        let base_url = Url::parse(&self.download_url.to_string())?;
        let url = base_url.join(&self.clone().get_full_compiler())?;

        let download: Download = Download::new(&url.to_string());
        let mut builder = Downloader::builder();
        builder
            .download_folder(std::path::Path::new(&self.compilers_path))
            .connect_timeout(Duration::from_secs(240));

        let mut downloader = builder.build()
            .map_err(|e| Error::msg(format!("Could not build downloader: {e}")))?;

        downloader.download(&[download])
            .map_err(|e| Error::msg(format!("Could not download zksolc compiler: {e}")));

        let compiler_path = self.compilers_path.join(self.clone().get_full_compiler());
        fs::set_permissions(&compiler_path, PermissionsExt::from_mode(0o755))
            .map_err(|e| Error::msg(format!("failed to set zksync compiler permissions: {e}")));

        Ok(())
    }
}
