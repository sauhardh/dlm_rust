use features::DownloadManager;
use serde::Deserialize;
use serde::Serialize;
use tokio;
use tokio::io::AsyncBufReadExt;
use tokio::net::UnixListener;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod features;
mod utils;

#[derive(Serialize, Deserialize)]
struct CommandsValue {
    command: String,
    urls: Option<Vec<String>>,
    id: Option<usize>,
}

#[inline]
fn create_req() -> PathBuf {
    let value = format!("/tmp/{:?}", env!("CARGO_PKG_NAME"));
    let dir_path = Path::new(&value);
    let socket_path = dir_path.join("SOCKET");

    if !Path::new(dir_path).exists() {
        fs::create_dir_all(dir_path).expect("Could not create a directory to establish UDS");
    }

    if socket_path.exists() {
        fs::remove_file(&socket_path).expect("Could not remove file");
    }
    socket_path
}

async fn start_socket() {
    let socket_path = create_req();
    let listener = UnixListener::bind(socket_path).expect("Failed to bind to the UDS LISTENER");
    let download_manager: Arc<Mutex<Option<DownloadManager>>> = Arc::new(Mutex::new(None));
    loop {
        match listener.accept().await {
            Ok((mut stream, _)) => {
                let mut reader = tokio::io::BufReader::new(&mut stream);
                let mut input = String::new();

                reader
                    .read_line(&mut input)
                    .await
                    .expect("Failed to read command");

                let commands: CommandsValue = serde_json::from_str(&input).unwrap();

                if let Some(urls) = commands.urls {
                    let dm = DownloadManager::new(urls);
                    *download_manager.lock().await = Some(dm);
                }

                match commands.command.as_str() {
                    "Download" => {
                        let dm = download_manager.lock().await.clone();
                        if let Some(dm) = dm {
                            tokio::spawn(async move {
                                dm.download().await;
                            });
                        }
                    }

                    "Pause" => {
                        let dm = download_manager.lock().await.clone();
                        if let Some(dm) = dm {
                            dm.pause_downloading(commands.id.unwrap()).await;
                        }
                    }

                    "Resume" => {
                        let dm = download_manager.lock().await.clone();
                        if let Some(dm) = dm {
                            dm.resume_download(commands.id.unwrap()).await;
                        }
                    }

                    "Cancel" => {
                        let dm = download_manager.lock().await.clone();
                        if let Some(dm) = dm {
                            dm.cancel_download(commands.id.unwrap()).await;
                        }
                    }
                    _ => {
                        println!("UnMatched Command Passed!");
                    }
                }
            }

            Err(e) => {
                println!("Error is: {e:?}");
            }
        }
    }
}

#[tokio::main]
async fn main() {
    start_socket().await;
}
