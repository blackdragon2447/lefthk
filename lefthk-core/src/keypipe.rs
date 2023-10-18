use crate::keyevent::KeyEvent;
use crate::errors::Result;
use std::path::{Path, PathBuf};
use tokio::{
    fs,
    io::{AsyncBufReadExt, BufReader},
    sync::mpsc,
};

pub struct KeyPipe {
    pipe_file: PathBuf,
    rx: mpsc::UnboundedReceiver<KeyEvent>,
}

impl Drop for KeyPipe {
    fn drop(&mut self) {
        use std::os::unix::fs::OpenOptionsExt;
        self.rx.close();

        // Open fifo for write to unblock pending open for read operation that prevents tokio runtime
        // from shutting down.
        let _unplock_pending_open = std::fs::OpenOptions::new()
            .write(true)
            .custom_flags(nix::fcntl::OFlag::O_NONBLOCK.bits())
            .open(self.pipe_file.clone());
    }
}

impl KeyPipe {
    /// Create and listen to the named pipe.
    /// # Errors
    ///
    /// Will error if unable to `mkfifo`, likely a filesystem issue
    /// such as inadequate permissions.
    pub async fn new(pipe_file: PathBuf) -> Result<Self> {
        let _pipe_reset = fs::remove_file(pipe_file.as_path()).await;
        nix::unistd::mkfifo(&pipe_file, nix::sys::stat::Mode::S_IRWXU)?;

        let path = pipe_file.clone();
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            while !tx.is_closed() {
                read_from_pipe(&path, &tx).await;
            }
            fs::remove_file(path).await.ok();
        });

        Ok(Self { pipe_file, rx })
    }

    pub fn pipe_name() -> PathBuf {
        let display = std::env::var("DISPLAY")
            .ok()
            .and_then(|d| d.rsplit_once(':').map(|(_, r)| r.to_owned()))
            .unwrap_or_else(|| "0".to_string());

        PathBuf::from(format!("keyevent-{}.pipe", display))
    }

    pub async fn get_next_event(&mut self) -> Option<KeyEvent> {
        if let Some(event) = self.rx.recv().await {
            return Some(event); 
        }
        None
    }
}

async fn read_from_pipe<'a>(pipe_file: &Path, tx: &mpsc::UnboundedSender<KeyEvent>) {
    if let Ok(file) = fs::File::open(pipe_file).await {
        let mut lines = BufReader::new(file).lines();

        while let Ok(line) = lines.next_line().await {
            if let Some(content) = line {
                if let Ok(event) = KeyEvent::try_from(content) {
                        if let Err(err) = tx.send(event) {
                            tracing::error!("{}", err);
                        }
                }
            }
        }
    }
}
