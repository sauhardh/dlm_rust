use features::DownloadManager;
use serde::Deserialize;
use serde::Serialize;
use tokio;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixListener;
use tokio::sync::Mutex;

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
    // PATH IS CURRENTLY HARDCODED.
    let dir_path = Path::new("/tmp/dlm_rust");
    let socket_path = dir_path.join("SOCKET");

    if !Path::new(dir_path).exists() {
        fs::create_dir_all(dir_path).expect("Could not create a directory to establish UDS");
    }

    if socket_path.exists() {
        fs::remove_file(&socket_path).expect("Could not remove file");
    }
    socket_path
}

async fn start_socket() -> Result<(), Box<dyn std::error::Error>> {
    let socket_path = create_req();
    let listener = UnixListener::bind(socket_path).expect("Failed to bind to the UDS LISTENER");
    let download_manager: Arc<Mutex<Option<DownloadManager>>> = Arc::new(Mutex::new(None));
    loop {
        match listener.accept().await {
            Ok((mut stream, _)) => {
                let mut reader = tokio::io::BufReader::new(&mut stream);
                let mut input = String::new();

                if reader.read_line(&mut input).await.is_err() {
                    println!("Failed to read command");
                    continue;
                }

                let commands: CommandsValue = match serde_json::from_str(&input) {
                    Ok(cmd) => cmd,
                    Err(err) => {
                        eprintln!("Invalid JSON command: {err}");
                        continue;
                    }
                };

                if let Some(urls) = commands.urls {
                    let dm = DownloadManager::new(urls);
                    *download_manager.lock().await = Some(dm);
                }

                match commands.command.as_str() {
                    "Download" => {
                        let dm = Arc::clone(&download_manager);
                        tokio::spawn(async move {
                            let dm = dm.lock().await.clone();
                            if let Some(dm) = dm {
                                let mut rx = dm.rx.lock().await;
                                while let Some(progress) = rx.recv().await {
                                    println!(
                                        "Received Progress: {:#?} and id: {:#?}",
                                        progress.progress, progress.id
                                    );

                                    let mut data = Vec::new();
                                    data.push(progress);
                                    let json_download = serde_json::to_string(&data).unwrap();

                                    stream.write_all(json_download.as_bytes()).await.unwrap();
                                    stream.write_all(b"\n").await.unwrap();
                                }
                            }
                        });

                        let dm = Arc::clone(&download_manager);
                        tokio::spawn(async move {
                            let dm = dm.lock().await.clone();
                            if let Some(dm) = dm {
                                dm.download().await;
                            }
                        });
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
                            dm.cancel_downloading(commands.id.unwrap()).await;
                        }
                    }
                    "List" => {
                        let dm = download_manager.clone().lock().await.clone();
                        tokio::spawn(async move {
                            // let dm = download_manager.lock().await.clone();
                            if let Some(dm) = dm {
                                let data = dm.list_downloads().await;
                                let json_data = serde_json::to_string(&data).unwrap();
                                println!("Json _ data is : {json_data:#?}");

                                let _ = stream.write_all(json_data.as_bytes()).await;
                                let _ = stream.write_all(b"\n").await;
                            }
                        });
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
    console_subscriber::init();

    if let Err(e) = start_socket().await {
        eprintln!("Error occured\n {:#?}", e);
    };
}
