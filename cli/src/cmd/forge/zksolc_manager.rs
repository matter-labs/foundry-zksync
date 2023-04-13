use dirs;
use anyhow::{Result, Error};
use downloader::{Downloader, Download};
use serde::Serialize;
use std::{fmt, fs, os::{unix::prelude::PermissionsExt}, path::PathBuf, time::Duration};
use url::Url;
use std::fs::File;
use std::io::copy;
use reqwest::blocking::Client;
use std::thread;

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

    fn get_download_uri(&self) -> &str {
        match self {
            ZkSolcOS::Linux => "linux-amd64",
            ZkSolcOS::Mac => "macosx-amd64",
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
            self.clone().get_full_download_url().unwrap(),
            self.clone().exists(),
        )
    }
}

impl ZkSolcManager {
    pub fn get_full_compiler(self) -> String {
        return format!("{}{}", self.compiler, self.version);
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
        let compiler_path = self.compilers_path.join(self.clone().get_full_compiler());
        compiler_path
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
        let url = self.clone().get_full_download_url()
            .map_err(|e| Error::msg(format!("Could not get full download url: {}", e)))?;

        let client = Client::new();
        let mut response = client.get(url)
            .send()
            .map_err(|e| Error::msg(format!("Failed to download file: {}", e)))?;

        let mut output_file = File::create(self.clone().get_full_compiler_path())
            .map_err(|e| Error::msg(format!("Failed to create output file: {}", e)))?;

        copy(&mut response, &mut output_file)
            .map_err(|e| Error::msg(format!("Failed to write the downloaded file: {}", e)))?;

        let compiler_path = self.compilers_path.join(self.clone().get_full_compiler());
        fs::set_permissions(&compiler_path, PermissionsExt::from_mode(0o755))
            .map_err(|e| Error::msg(format!("failed to set zksync compiler permissions: {e}")));

        Ok(())
    }
}

// https://github.com/matter-labs/zksolc-bin/raw/main/linux-amd64/zksolc-linux-amd64-musl-v1.3.0
// https://github.com/matter-labs/zksolc-bin/raw/main/zksolc-linux-amd64-musl-v1.3.8
