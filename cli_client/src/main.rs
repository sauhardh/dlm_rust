use clap::Parser;
use serde::Deserialize;
use serde::Serialize;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;

use std::path::Path;

mod cli;

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
            println!("Downloading url : {:?}", urls);

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
            println!("Resuming of id : {:?}", id);
            return Ok(CommandsValue {
                command: "Resume".to_string(),
                urls: None,
                id: Some(id),
            });
        }
        Commands::Cancel { id } => {
            println!("Canceling of id : {:?}", id);
            return Ok(CommandsValue {
                command: "Cancel".to_string(),
                urls: None,
                id: Some(id),
            });
        }
        Commands::List => {
            println!("List all the downloads");
            return Ok(CommandsValue {
                command: "List".to_string(),
                urls: None,
                id: None,
            });
        }
    }
}

async fn connect_send_socket(command: CommandsValue) -> Result<(), Box<dyn std::error::Error>> {
    let value = format!("/tmp/{:?}", env!("CARGO_PKG_NAME"));
    let dir_path = Path::new(&value);
    let socket_path = dir_path.join("SOCKET");

    let mut stream: UnixStream = UnixStream::connect(socket_path).await?;
    let command_str = serde_json::to_string(&command)?;
    stream.write_all(command_str.as_bytes()).await?;
    stream.write_all(b"\n").await?;
    stream.flush().await?;

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
