use features::DownloadManager;
use serde::Deserialize;
use serde::Serialize;
use tokio;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixListener;
use tokio::sync::Mutex;
use tracing::{error, warn};
use tracing_subscriber;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

mod features;
mod utils;

#[derive(Serialize, Deserialize, Debug)]
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

#[derive(Clone)]
struct SharedState {
    download_manager: Arc<Mutex<DownloadManager>>,
}

impl SharedState {
    pub fn new() -> Self {
        Self {
            download_manager: Arc::new(Mutex::new(DownloadManager::new())),
        }
    }

    pub async fn handle_connection(self, stream: tokio::net::UnixStream) {
        let (mut reader_half, writer_half) = stream.into_split();
        let mut reader = tokio::io::BufReader::new(&mut reader_half);
        let mut input = String::new();

        let writer = Arc::new(Mutex::new(writer_half));

        while reader.read_line(&mut input).await.is_ok() {
            if let Ok(commands) = serde_json::from_str::<CommandsValue>(&input) {
                match commands.command.as_str() {
                    "Download" => {
                        let dm = Arc::clone(&self.download_manager);
                        if let Some(urls) = commands.urls {
                            let mut dm_lock = dm.lock().await;

                            let dm = &mut *dm_lock;
                            dm.add_urls(urls).await;
                        }

                        tokio::spawn(async move {
                            let dm = dm.lock().await.clone();
                            dm.download().await;
                        });

                        let dm = Arc::clone(&self.download_manager);
                        let download_writer = Arc::clone(&writer);

                        // To send the progress back to the client
                        tokio::spawn(async move {
                            let dm = dm.lock().await.clone();
                            let mut rx = dm.rx.lock().await;
                            while let Some(progress) = rx.recv().await {
                                let mut data = vec![progress];
                                let json_download = serde_json::to_string(&data).unwrap();
                                data.clear();

                                if let Err(e) = download_writer
                                    .lock()
                                    .await
                                    .write_all(json_download.as_bytes())
                                    .await
                                {
                                    error!("Error occured on sending download info: {e:#?}");
                                    break;
                                };

                                if let Err(e) = download_writer.lock().await.write_all(b"\n").await
                                {
                                    error!("Error occured on sending end line for download info: {e:#?}");
                                    break;
                                };
                            }
                        });
                    }

                    "Pause" => {
                        let dm = self.download_manager.lock().await.clone();
                        dm.pause_downloading(commands.id.unwrap()).await;
                    }

                    "Resume" => {
                        let dm = self.download_manager.lock().await.clone();
                        dm.resume_download(commands.id.unwrap()).await;
                    }

                    "Cancel" => {
                        let dm = self.download_manager.lock().await.clone();
                        dm.cancel_downloading(commands.id.unwrap()).await;
                    }
                    "List" => {
                        let dm = self.download_manager.clone().lock().await.clone();
                        let list_writer = Arc::clone(&writer);

                        //List all the downloads
                        tokio::spawn(async move {
                            let data = dm.list_downloads().await;
                            let json_data = serde_json::to_string(&data).unwrap();

                            let _ = list_writer
                                .lock()
                                .await
                                .write_all(json_data.as_bytes())
                                .await;
                            let _ = list_writer.lock().await.write_all(b"\n").await;
                        });
                    }
                    _ => {
                        warn!("Unmatched Command Passed!");
                    }
                }
            }
            input.clear();
        }
    }
}

#[tokio::main]
async fn main() {
    // Uncomment this if you want to use tokio-console.
    // console_subscriber::init();

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let state = SharedState::new();
    let listener = UnixListener::bind(create_req()).expect("Failed to bind to the UDS LISTENER");

    while let Ok((stream, _)) = listener.accept().await {
        let state = state.clone();
        state.handle_connection(stream).await;
    }
}
