#  dlm_rust (Download Manager) 


A TUI-based asynchronous Download Manager that leverages the power of IPC.  
Built with `ratatui` for the terminal interface and `tokio-console` for async task monitoring.

---

## 📝 NOTE

> Currently only supports **UNIX** systems due to the use of `Unix Domain Socket` for IPC.  
> Windows support via `named_pipe` is planned. *(Work In Progress)*. <br/>
> Uncomment this `console_subscriber::init();` on `server/src/main.rs` to use tokio-console.

- **`backend` directory**: Contains core downloading logic (pausing/resuming via thread locking).  
- **`View` directory**: Handles the TUI interface.


---

## ✨ Features

- 🖥️ Intuitive Terminal User Interface (`ratatui`)  
- 🔍 Async task debugging with `tokio-console`  
- ⚡ Concurrent multi-link downloads  
- ⏸️ Pause/Resume functionality  
- 📋 Download listing 
- ⏲ Retry upto 2 times if downloading fails.

---

## 🚧 TODO

- [ ] Windows support via `named_pipe`  
- [ ] Error handling

---

## 🛠️ Run

### Clone & Navigate
```bash
git clone <repo-link>
cd dlm_rust
```
### Mannual Execution

**1) Run server first**
```bash
cargo run
```

**2) Run client(tui)**

```bash
cd client
```

```bash
cargo run
```



### Example

!["example"](./assets/example.png)

*multi_download*

!["example2](./assets/example2.png)