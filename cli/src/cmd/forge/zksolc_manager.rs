use dirs;
use anyhow::{Result, Error};
use serde::Serialize;
use std::{fmt, fs, os::{unix::prelude::PermissionsExt}, path::PathBuf};
use url::Url;
use std::fs::File;
use std::io::copy;
use reqwest::blocking::Client;

#[derive(Debug, Clone, Serialize)]
pub enum ZkSolcVersion {
    V135,
    V136,
    V137,
    V138,
}

fn parse_version(version: &str) -> Result<ZkSolcVersion> {
    match version {
        "v1.3.5" => Ok(ZkSolcVersion::V135),
        "v1.3.6" => Ok(ZkSolcVersion::V136),
        "v1.3.7" => Ok(ZkSolcVersion::V137),
        "v1.3.8" => Ok(ZkSolcVersion::V138),
        _ => Err(Error::msg("Unsupported version")),
    }
}

impl ZkSolcVersion {
    fn get_version(&self) -> Result<&str> {
        match self {
            ZkSolcVersion::V135 => Ok("v1.3.5"),
            ZkSolcVersion::V136 => Ok("v1.3.6"),
            ZkSolcVersion::V137 => Ok("v1.3.7"),
            ZkSolcVersion::V138 => Ok("v1.3.8"),
            _ => Err(Error::msg("ZkSolc compiler version not supported")),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
enum ZkSolcOS {
    Linux,
    Mac,
}

fn get_operating_system() -> Result<ZkSolcOS> {
    match std::env::consts::OS {
        "linux" => Ok(ZkSolcOS::Linux),
        "macos" | "darwin" => Ok(ZkSolcOS::Mac),
        _ => Err(Error::msg("Unsupported opeating system")),
    }
}

impl ZkSolcOS {
    fn get_compiler(&self) -> &str {
        match self {
            ZkSolcOS::Linux => "zksolc-linux-amd64-musl-",
            ZkSolcOS::Mac => "zksolc-macosx-arm64-",
        }
    }

    fn get_download_uri(&self) -> &str {
        match self {
            ZkSolcOS::Linux => "linux-amd64",
            ZkSolcOS::Mac => "macosx-amd64",
        }
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
            download_url: Url::parse("https://github.com/matter-labs/zksolc-bin/raw/main").unwrap(),
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

            if let Ok(solc_version) = parse_version(&version) {
                return Ok(ZkSolcManager {
                    compilers_path,
                    version: solc_version,
                    compiler,
                    download_url,
                });
            }
        }
        Err(Error::msg("Could not build ZkSolcManager"))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ZkSolcManager {
    pub compilers_path: PathBuf,
    pub version: ZkSolcVersion,
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
            self.version.get_version().unwrap(), 
            self.compiler, 
            self.download_url, 
            self.clone().get_full_compiler(), 
            self.clone().get_full_download_url().unwrap(),
            self.clone().exists(),
        )
    }
}

impl ZkSolcManager {
    pub fn get_full_compiler(self) -> String {
        return format!("{}{}", self.compiler, self.version.get_version().unwrap());
    }

    pub fn get_full_download_url(&self) -> Result<Url> {
        if let Ok(zk_solc_os) = get_operating_system() {
            let download_uri = zk_solc_os.get_download_uri().to_string();
            let full_download_url = format!("{}/{}/{}", self.download_url, download_uri, self.clone().get_full_compiler());
            if let Ok(url) = Url::parse(&full_download_url) {
                Ok(url)
            } else {
                Err(Error::msg("Could not parse full download url"))
            }
        } else {
            Err(Error::msg("Could not determine full download url"))
        }
    }

    pub fn get_full_compiler_path(&self) -> PathBuf {
        self.compilers_path.join(self.clone().get_full_compiler())
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

    pub fn download(self) -> Result<()> {
        if self.exists() {
            // TODO: figure out better don't download if compiler is downloaded
            return Ok(())
        }
        
        let url = self.get_full_download_url()
            .map_err(|e| Error::msg(format!("Could not get full download url: {}", e)))?;

        let client = Client::new();
        let mut response = client.get(url)
            .send()
            .map_err(|e| Error::msg(format!("Failed to download file: {}", e)))?;

        let mut output_file = File::create(self.get_full_compiler_path())
            .map_err(|e| Error::msg(format!("Failed to create output file: {}", e)))?;

        copy(&mut response, &mut output_file)
            .map_err(|e| Error::msg(format!("Failed to write the downloaded file: {}", e)))?;

        let compiler_path = self.compilers_path.join(self.clone().get_full_compiler());
        let _ = fs::set_permissions(compiler_path, PermissionsExt::from_mode(0o755))
            .map_err(|e| Error::msg(format!("Failed to set zksync compiler permissions: {e}")));

        Ok(())
    }
}

// https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.3.0
// https://github.com/matter-labs/zksolc-bin/raw/main/zksolc-linux-amd64-musl-v1.3.8
