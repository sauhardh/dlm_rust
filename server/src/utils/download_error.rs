use reqwest;

#[derive(Debug)]
#[allow(dead_code)]
pub enum DownloadError {
    ReqwestError(reqwest::Error),
    IoError(std::io::Error),
    Other(String),
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DownloadError::ReqwestError(e) => write!(f, "Reqwest : {:#?}", e),
            DownloadError::IoError(e) => write!(f, "Io Error: {:#?}", e),
            DownloadError::Other(e) => write!(f, "Error occured: {:#?}", e),
        }
    }
}

impl std::error::Error for DownloadError {}

impl From<reqwest::Error> for DownloadError {
    fn from(value: reqwest::Error) -> Self {
        DownloadError::ReqwestError(value)
    }
}

impl From<std::io::Error> for DownloadError {
    fn from(value: std::io::Error) -> Self {
        DownloadError::IoError(value)
    }
}
