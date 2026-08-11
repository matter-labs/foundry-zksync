use std::str::FromStr;

///
/// The compiler warning type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WarningType {
    /// The eponymous feature.
    TxOrigin,
    /// The eponymous feature.
    AssemblyCreate,
}

impl WarningType {
    ///
    /// Converts string arguments into an array of warnings.
    pub fn try_from_strings(strings: &[String]) -> Result<Vec<Self>, eyre::Error> {
        strings.iter().map(|string| Self::from_str(string)).collect()
    }
}

impl FromStr for WarningType {
    type Err = eyre::Error;

    fn from_str(string: &str) -> Result<Self, Self::Err> {
        match string {
            "txorigin" => Ok(Self::TxOrigin),
            "assemblycreate" => Ok(Self::AssemblyCreate),
            r#type => Err(eyre::eyre!("Invalid suppressed warning type: {type}")),
        }
    }
}

impl std::fmt::Display for WarningType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::TxOrigin => write!(f, "txorigin"),
            Self::AssemblyCreate => write!(f, "assemblycreate"),
        }
    }
}

///
/// The compiler error type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ErrorType {
    /// The eponymous feature.
    SendTransfer,
    /// The eponymous feature.
    Ripemd160,
}

impl ErrorType {
    ///
    /// Converts string arguments into an array of errors.
    pub fn try_from_strings(strings: &[String]) -> Result<Vec<Self>, eyre::Error> {
        strings.iter().map(|string| Self::from_str(string)).collect()
    }
}

impl FromStr for ErrorType {
    type Err = eyre::Error;

    fn from_str(string: &str) -> Result<Self, Self::Err> {
        match string {
            "sendtransfer" => Ok(Self::SendTransfer),
            "ripemd160" => Ok(Self::Ripemd160),
            r#type => Err(eyre::eyre!("Invalid suppressed error type: {type}")),
        }
    }
}

impl std::fmt::Display for ErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::SendTransfer => write!(f, "sendtransfer"),
            Self::Ripemd160 => write!(f, "ripemd160"),
        }
    }
}
