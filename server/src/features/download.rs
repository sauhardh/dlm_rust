use futures::StreamExt;
use reqwest::Client;
use serde::Serialize;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::Mutex;
use tokio::sync::Notify;
use tokio::sync::Semaphore;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::utils::filter_name;
use crate::utils::os_download_dir;
use crate::utils::validate_url;
use crate::utils::DownloadError;

#[derive(PartialEq, Clone, Debug, Serialize)]
pub enum State {
    Downloading,
    Paused,
    Completed,
    Canceled,
    Pending,
}

#[derive(Clone, Debug, Serialize)]
pub struct SingleDownload {
    pub id: usize,
    pub progress: usize,
    url: String,
    total_length: usize,
    #[serde(skip_serializing)]
    client: Client,
    destination: PathBuf,
    #[serde(skip_serializing)]
    notify: Arc<Notify>,
    state: State,
    #[serde(skip_serializing)]
    tx: UnboundedSender<SingleDownload>,
}

#[allow(dead_code)]
impl SingleDownload {
    pub fn new(url: &str, id: usize, tx: UnboundedSender<SingleDownload>) -> Self {
        SingleDownload {
            id,
            progress: 0,
            url: url.to_string(),
            total_length: 0,
            client: Client::new(),
            destination: os_download_dir().join(filter_name(url.to_string())),
            notify: Arc::new(Notify::new()),
            state: State::Pending,
            tx,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DownloadManager {
    no_of_downloads: usize,
    infos: HashMap<usize, Arc<Mutex<SingleDownload>>>,
    pub rx: Arc<Mutex<UnboundedReceiver<SingleDownload>>>,
    tx: UnboundedSender<SingleDownload>,
    urls: Vec<String>,
}

impl DownloadManager {
    pub fn new() -> Self {
        let (tx, rx) = unbounded_channel();

        DownloadManager {
            no_of_downloads: 0,
            infos: HashMap::new(),
            rx: Arc::new(Mutex::new(rx)),
            urls: Vec::new(),
            tx,
        }
    }

    pub fn add_urls(&mut self, urls: Vec<String>) {
        let base_id = self.no_of_downloads;
        for (_, url) in urls.iter().enumerate() {
            if let Err(e) = validate_url(&url) {
                eprintln!("Failed to validate the url:{url}.\nMore: {e:#?}");
                continue;
            }

            if self.urls.contains(url) {
                println!("URL already downloading.");
                continue;
            }

            self.urls.push(url.to_string());
            let id = base_id + 1;

            self.infos.insert(
                id,
                Arc::new(Mutex::new(SingleDownload::new(url, id, self.tx.clone()))),
            );
        }
        self.no_of_downloads = self.urls.len();
    }

    pub async fn pause_downloading(&self, id: usize) {
        for info in &self.infos {
            let mut locked_info = info.1.lock().await;
            if locked_info.id == id {
                locked_info.state = State::Paused;
                self.send_back_progress(locked_info).await;
                break;
            }
        }
    }

    pub async fn resume_download(&self, id: usize) {
        for info in &self.infos {
            let mut locked_info = info.1.lock().await;
            if locked_info.id == id && locked_info.state == State::Paused {
                locked_info.state = State::Downloading;
                locked_info.notify.notify_one();
                break;
            }
        }
    }

    pub async fn cancel_downloading(&self, id: usize) {
        for info in &self.infos {
            let mut locked_info = info.1.lock().await;
            if locked_info.id == id {
                locked_info.state = State::Canceled;
                self.send_back_progress(locked_info).await;
                break;
            }
        }
    }

    pub async fn list_downloads(self) -> Vec<SingleDownload> {
        let mut vec = Vec::new();
        for info in &self.infos {
            let locked_info = info.1.lock().await;
            vec.push(locked_info.clone());
        }

        vec
    }

    /// This send the current progress info i.e. [SingleDownload] to the client
    ///
    /// info that is locked and passed to the function is droped.
    async fn send_back_progress(&self, info: tokio::sync::MutexGuard<'_, SingleDownload>) {
        if let Err(e) = info.tx.send(info.clone()) {
            eprintln!("Failed to pass the message through channel. \n Info: {e}");
        }
        drop(info);
    }

    #[inline]
    /// Make http request and download the data
    async fn single_download(
        &self,
        single_info: Arc<Mutex<SingleDownload>>,
    ) -> Result<(), DownloadError> {
        let mut info = single_info.lock().await;
        let mut downloaded = info.progress;

        let http_request = info.client.get(&info.url);
        let http_response = http_request.send().await?;

        info.total_length = http_response.content_length().unwrap_or(0) as usize;
        info.state = State::Downloading;

        let mut stream = http_response.bytes_stream();
        let mut file = BufWriter::with_capacity(
            1024 * 1024,
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&info.destination)
                .await?,
        );

        drop(info);

        while let Some(chunk) = stream.next().await {
            let notify = {
                let info = single_info.lock().await;
                if info.state == State::Paused {
                    println!("Downloading paused; {:?}", info.id);
                    Some(info.notify.clone())
                } else {
                    None
                }
            };

            if let Some(notify) = notify {
                notify.notified().await;
            }

            {
                let info = single_info.lock().await;
                if info.state == State::Canceled {
                    drop(info);
                    break;
                }
            }

            let chunk = chunk?;
            file.write_all(&chunk).await?;
            downloaded += chunk.len();

            // To Send the Progress.
            let mut info = single_info.lock().await;
            info.progress = downloaded;
            self.send_back_progress(info).await;
        }

        // After completion of downloading.
        let mut info = single_info.lock().await;
        info.state = State::Completed;
        self.send_back_progress(info).await;

        file.flush().await?;

        Ok(())
    }

    /// Retry downloading if error occurs.
    ///
    /// Retry upto 2 times.
    #[inline]
    async fn attempt_download(
        &self,
        single_info: Arc<Mutex<SingleDownload>>,
    ) -> Result<(), DownloadError> {
        let mut retries = 2;
        let mut last_error = None;

        while retries > 0 {
            match self.single_download(Arc::clone(&single_info)).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    eprintln!("\t__Try number: {retries}__\t");
                    last_error = Some(e);
                    retries -= 1;
                    tokio::time::sleep(Duration::from_millis(40)).await;
                }
            }
        }

        Err(last_error.unwrap())
    }

    /// Public Download Function
    ///
    /// async download the data from list of urls
    pub async fn download(&self) {
        let semaphore = Arc::new(Semaphore::new(10));

        let mut tasks = Vec::new();
        for single_info in &self.infos {
            let single_info = Arc::clone(single_info.1);
            let permit = semaphore.clone().acquire_owned().await.unwrap();

            let this = self.clone();

            tasks.push(tokio::spawn(async move {
                if let Err(err) = this.attempt_download(single_info).await {
                    eprintln!("Failed to download the request.\nMore: {err:#?}");
                }

                drop(permit);
            }));
        }

        for task in tasks {
            task.await.unwrap();
        }
    }
}
