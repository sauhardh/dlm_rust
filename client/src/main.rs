use serde::Deserialize;
use serde::Serialize;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::UnboundedSender;
use tui::CommandTab;

use std::error::Error;
use std::path::Path;
use std::path::PathBuf;

mod tui;

#[derive(Serialize, Deserialize, Debug)]
pub struct CommandArgument {
    command: CommandTab,
    urls: Option<Vec<String>>,
    id: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SingleDownload {
    id: usize,
    progress: usize,
    url: String,
    total_length: usize,
    destination: PathBuf,
    state: String,
}

pub async fn connect_socket() -> Result<UnixStream, Box<dyn std::error::Error>> {
    let socket_path = Path::new("/tmp/dlm_rust").join("SOCKET");
    let stream: UnixStream = UnixStream::connect(&socket_path).await?;

    Ok(stream)
}

pub async fn send_command(
    mut write_half: tokio::net::unix::OwnedWriteHalf,
    mut commands_rx: UnboundedReceiver<CommandArgument>,
) -> Result<(), Box<dyn Error>> {
    let mut buffer = Vec::new();

    while let Some(command) = commands_rx.recv().await {
        serde_json::to_writer(&mut buffer, &command)?;
        buffer.push(b'\n');
        write_half.write_all(&buffer).await?;

        write_half.flush().await?;
        buffer.clear();
    }

    Ok(())
}

pub async fn receive_progress(
    read_half: tokio::net::unix::OwnedReadHalf,
    realtime_tx: UnboundedSender<SingleDownload>,
) -> Result<(), Box<dyn Error>> {
    let mut reader = BufReader::new(read_half);
    let mut line: String = String::new();

    while let Ok(n) = reader.read_line(&mut line).await {
        if n == 0 {
            break;
        }

        match serde_json::from_str::<Vec<SingleDownload>>(&line) {
            Ok(data) => {
                for each in data {
                    if let Err(e) = realtime_tx.send(each) {
                        eprintln!("Error occurent while sending progress through the channel:{e}");
                        break;
                    }
                }
            }
            Err(e) => {
                eprintln!("Deserialization Error: {e:#?}");
            }
        }
        line.clear();
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    match connect_socket().await {
        Ok(stream) => {
            let (read_half, write_half) = stream.into_split();

            let (realtime_tx, realtime_rx) = mpsc::unbounded_channel::<SingleDownload>();
            let (command_tx, command_rx) = mpsc::unbounded_channel::<CommandArgument>();

            tokio::spawn(async move {
                if let Err(e) = receive_progress(read_half, realtime_tx).await {
                    eprintln!("Failed to receive progress: {e}");
                };
            });

            tokio::spawn(async move {
                if let Err(e) = send_command(write_half, command_rx).await {
                    eprintln!("Failed to send command: {e}");
                }
            });

            if let Err(e) = tui::run_tui(command_tx, realtime_rx).await {
                println!("Failed to run TUI: {:#?}", e);
            }
        }
        Err(e) => eprintln!("The error is occured while connecting : {e:#?}"),
    }
}
