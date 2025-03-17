use futures::StreamExt;
use reqwest::Client;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::Mutex;
use tokio::sync::Notify;
use tokio::sync::Semaphore;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::utils::filter_name;
use crate::utils::os_download_dir;
use crate::utils::validate_url;
use crate::utils::DownloadError;

#[derive(PartialEq, Clone, Debug)]
enum State {
    Downloading,
    Paused,
    Completed,
    Canceled,
    Pending,
}

#[derive(Clone, Debug)]
struct SingleDownload {
    id: usize,
    progress: usize,
    url: String,
    total_length: usize,
    client: Client,
    destination: PathBuf,
    notify: Arc<Notify>,
    state: State,
    tx: UnboundedSender<usize>,
}

#[allow(dead_code)]
impl SingleDownload {
    pub fn new(url: &str, id: usize, tx: UnboundedSender<usize>) -> Self {
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
    infos: Vec<Arc<Mutex<SingleDownload>>>,
    rx: Arc<Mutex<UnboundedReceiver<usize>>>,
}

impl DownloadManager {
    pub fn new(urls: Vec<String>) -> Self {
        let (tx, rx) = unbounded_channel();

        let mut infos = Vec::new();
        for (id, url) in urls.iter().enumerate() {
            if let Err(e) = validate_url(&url) {
                eprintln!("Failed to validate the url:{url}.\nMore: {e:#?}");
                continue;
            }
            infos.push(Arc::new(Mutex::new(SingleDownload::new(
                url,
                id,
                tx.clone(),
            ))));
        }

        DownloadManager {
            no_of_downloads: infos.len(),
            infos,
            rx: Arc::new(Mutex::new(rx)),
        }
    }

    pub async fn pause_downloading(&self, id: usize) {
        for info in &self.infos {
            let mut locked_info = info.lock().await;
            if locked_info.id == id {
                locked_info.state = State::Paused;
                break;
            }
        }
    }

    #[inline]
    /// Make http request and download the data
    async fn single_download(
        &self,
        single_info: Arc<Mutex<SingleDownload>>,
    ) -> Result<(), DownloadError> {
        let mut downloaded;
        let mut stream;
        let mut file;
        let tx: UnboundedSender<usize>;
        {
            let mut info = single_info.lock().await;
            downloaded = info.progress;

            let mut http_request = info.client.get(&info.url);
            if downloaded > 0 {
                println!("yes in range and downloaded is : {downloaded}");
                http_request = http_request.header("Range", format!("bytes={}-", downloaded));
            }
            let http_response = http_request.send().await?;

            info.total_length = http_response.content_length().unwrap_or(0) as usize;

            stream = http_response.bytes_stream();
            file = BufWriter::with_capacity(
                1024 * 1024,
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&info.destination)
                    .await?,
            );

            tx = info.tx.clone();
        }

        while let Some(chunk) = stream.next().await {
            {
                let info = single_info.lock().await;
                if info.state == State::Paused {
                    println!("Downloading paused");
                    info.notify.notified().await;
                }

                if info.state == State::Canceled {
                    println!("Downloading cancelled");
                    break;
                }
            }

            let chunk = chunk?;
            downloaded += chunk.len();

            file.write_all(&chunk).await?;
            println!("Written : {downloaded:?}");

            if let Err(e) = tx.send(downloaded) {
                eprintln!("Failed to pass the message through channel.\nInfo: {e}")
            }
        }

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
    pub async fn download(self) {
        let semaphore = Arc::new(Semaphore::new(10));

        let mut tasks = Vec::new();
        for single_info in &self.infos {
            let single_info = Arc::clone(single_info);
            let permit = semaphore.clone().acquire_owned().await.unwrap();

            let this = self.clone();

            tasks.push(tokio::spawn(async move {
                if let Err(err) = this.attempt_download(single_info).await {
                    eprintln!("Failed to download the request.\nMore: {err:#?}");
                }

                println!("drop");
                drop(permit);
            }));
        }

        let _ = tokio::spawn(async move {
            let mut rx = self.rx.lock().await;
            while let Some(progress) = rx.recv().await {
                println!("Received Progress: {progress:?}")
            }
        });

        for task in tasks {
            task.await.unwrap();
        }
    }
}
