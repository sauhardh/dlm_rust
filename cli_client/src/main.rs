use clap::Parser;
use serde::Deserialize;
use serde::Serialize;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::net::UnixStream;

use std::path::Path;
use std::path::PathBuf;

mod cli;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SingleDownload {
    id: usize,
    progress: usize,
    url: String,
    total_length: usize,
    destination: PathBuf,
    state: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct CommandsValue {
    command: String,
    urls: Option<Vec<String>>,
    id: Option<usize>,
}

/// Parse the CLI arguments and executes the commands
pub(crate) async fn parse_args() -> Result<CommandsValue, Box<dyn std::error::Error>> {
    let args = cli::Args::parse();

    use cli::Commands;
    match args.command {
        Commands::Download { urls } => {
            if urls.is_empty() {
                return Err(format!(
                    "Empty field provided. Expected at least one argument, Got {:?}",
                    urls.len()
                )
                .into());
            }

            return Ok(CommandsValue {
                command: "Download".to_string(),
                urls: Some(urls),
                id: None,
            });
        }
        Commands::Pause { id } => {
            return Ok(CommandsValue {
                command: "Pause".to_string(),
                urls: None,
                id: Some(id),
            });
        }
        Commands::Resume { id } => {
            return Ok(CommandsValue {
                command: "Resume".to_string(),
                urls: None,
                id: Some(id),
            });
        }
        Commands::Cancel { id } => {
            return Ok(CommandsValue {
                command: "Cancel".to_string(),
                urls: None,
                id: Some(id),
            });
        }
        Commands::List => {
            return Ok(CommandsValue {
                command: "List".to_string(),
                urls: None,
                id: None,
            });
        }
    }
}

async fn connect_send_socket(command: CommandsValue) -> Result<(), Box<dyn std::error::Error>> {
    // PATH IS CURRENTLY HARDCODED.
    let socket_path = Path::new("/tmp/dlm_rust").join("SOCKET");

    let mut stream: UnixStream = UnixStream::connect(socket_path).await?;
    let command_str = serde_json::to_string(&command)?;
    stream.write_all(command_str.as_bytes()).await?;
    stream.write_all(b"\n").await?;
    stream.flush().await?;

    let mut buffer = String::new();
    let mut reader = BufReader::new(&mut stream);
    reader.read_line(&mut buffer).await.unwrap();

    if buffer.len() > 0 {
        let buffer = buffer.trim();

        match serde_json::from_str::<Vec<SingleDownload>>(&buffer) {
            Ok(json_value) => {
                println!(
                    "No of downloads: {:#?} \n\n Downloads Info : {:#?}",
                    json_value.len(),
                    json_value
                );
            }
            Err(e) => {
                eprintln!("Failed to parse the JSON response: {e}");
                return Err(Box::new(e));
            }
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    if let Ok(command) = parse_args().await {
        if let Err(e) = connect_send_socket(command).await {
            eprintln!("Error occured while connecting and sending to the socket\nInfo: {e:#?}")
        };
    }
}
