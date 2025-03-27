
# DLM

⚠️ work in progress

 A TUI based asynchronous Download Manager that levarages the power of IPC.
 Uses `ratatui` for tui, `tokio-console` to console the async task.

  

###  NOTE:

> Currently It only supports for `UNIX` system because of the `Unix Domain Socket` IPC, which is used. For windows, `named_pipe` could be the choice to use. (Work In Progress).

> `server` directory contains main downloading, pausing , resuming code. pausing, resuming can be done only while downloading, by using thread lock.
> `client` directory contains the TUI logic. yet to reach the best version of it.
  
  

## Features

- User intutive Terminal User Interface
- Able to download multiple links concurrently.
- Able to Pause, Resume, List the download.

  

## TODO:

[] Make it able to pause, resume from the downloader itself (currently inside server folder).
[] Make it able to pause when downloading from the `TUI`.
[] Make it work on windows through `named_pipe` (That's the current approach)


## Run

```
git clone <Link of this repo>
```
```
cd dlm_rust
```

**Mannual Running**

*It runs the server*
```
cargo run
```
<br>

*To run the TUI*
```
cd client
```
```
cargo run
```

<br>

**or, Use bash**

you may need to, 

```
chmod +x run.sh
```

```
./run.sh
```

