use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};

#[cfg(not(unix))]
use std::net::{TcpListener, TcpStream};

/// Generates a deterministic socket / lock path for the given index directory.
/// Scoped to the current user and index path hash.
pub fn get_endpoint_path(index_path: &str) -> PathBuf {
    if let Ok(custom) = std::env::var("DOCSEARCH_SOCKET_PATH") {
        return PathBuf::from(custom);
    }

    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(index_path.as_bytes());
    let hash = hex::encode(&hasher.finalize()[..8]);

    #[cfg(unix)]
    {
        if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
            let p = PathBuf::from(runtime_dir).join(format!("docsearch-{}.sock", hash));
            if let Some(parent) = p.parent() {
                if parent.is_dir() {
                    return p;
                }
            }
        }
    }

    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "default".to_string());

    #[cfg(unix)]
    {
        std::env::temp_dir().join(format!("docsearch-{}-{}.sock", user, hash))
    }
    #[cfg(not(unix))]
    {
        std::env::temp_dir().join(format!("docsearch-{}-{}.port", user, hash))
    }
}

pub struct SingleInstanceClient {
    endpoint: PathBuf,
}

impl SingleInstanceClient {
    pub fn new(endpoint: PathBuf) -> Self {
        Self { endpoint }
    }

    /// Sends a command string (e.g. "SHOW", "RESCAN", "PING") and awaits the response.
    pub fn send_action(&mut self, action: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        #[cfg(unix)]
        {
            let mut stream = UnixStream::connect(&self.endpoint)?;
            stream.set_read_timeout(Some(Duration::from_millis(2000)))?;
            stream.set_write_timeout(Some(Duration::from_millis(2000)))?;

            writeln!(stream, "{}", action)?;
            stream.flush()?;

            let mut reader = BufReader::new(stream);
            let mut response = String::new();
            reader.read_line(&mut response)?;
            Ok(response.trim().to_string())
        }
        #[cfg(not(unix))]
        {
            let port_str = std::fs::read_to_string(&self.endpoint)?;
            let port: u16 = port_str.trim().parse()?;
            let mut stream = TcpStream::connect(("127.0.0.1", port))?;
            stream.set_read_timeout(Some(Duration::from_millis(2000)))?;
            stream.set_write_timeout(Some(Duration::from_millis(2000)))?;

            writeln!(stream, "{}", action)?;
            stream.flush()?;

            let mut reader = BufReader::new(stream);
            let mut response = String::new();
            reader.read_line(&mut response)?;
            Ok(response.trim().to_string())
        }
    }
}

pub struct SingleInstanceServer {
    endpoint: PathBuf,
    #[cfg(unix)]
    listener: UnixListener,
    #[cfg(not(unix))]
    listener: TcpListener,
}

/// Guard handle that removes the endpoint socket/file upon drop (application shutdown).
pub struct SingleInstanceHandle {
    endpoint: PathBuf,
}

impl Drop for SingleInstanceHandle {
    fn drop(&mut self) {
        if self.endpoint.exists() {
            let _ = std::fs::remove_file(&self.endpoint);
        }
    }
}

impl SingleInstanceServer {
    /// Starts background thread to listen for IPC commands from new instances.
    pub fn start_listener<F>(self, on_action: F) -> SingleInstanceHandle
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        let endpoint = self.endpoint.clone();
        let handle = SingleInstanceHandle {
            endpoint: endpoint.clone(),
        };

        std::thread::Builder::new()
            .name("docsearch-ipc".to_string())
            .spawn(move || {
                #[cfg(unix)]
                {
                    for stream in self.listener.incoming() {
                        match stream {
                            Ok(mut s) => {
                                let _ = s.set_read_timeout(Some(Duration::from_millis(2000)));
                                let _ = s.set_write_timeout(Some(Duration::from_millis(2000)));
                                let stream_clone = match s.try_clone() {
                                    Ok(c) => c,
                                    Err(_) => continue,
                                };
                                let mut reader = BufReader::new(stream_clone);
                                let mut line = String::new();
                                if reader.read_line(&mut line).is_ok() {
                                    let cmd = line.trim();
                                    match cmd {
                                        "PING" => {
                                            let _ = writeln!(s, "PONG");
                                            let _ = s.flush();
                                        }
                                        "SHOW" => {
                                            on_action("SHOW");
                                            let _ = writeln!(s, "OK");
                                            let _ = s.flush();
                                        }
                                        "RESCAN" => {
                                            on_action("RESCAN");
                                            let _ = writeln!(s, "OK");
                                            let _ = s.flush();
                                        }
                                        other => {
                                            on_action(other);
                                            let _ = writeln!(s, "OK");
                                            let _ = s.flush();
                                        }
                                    }
                                }
                            }
                            Err(_) => break,
                        }
                    }
                }
                #[cfg(not(unix))]
                {
                    for stream in self.listener.incoming() {
                        match stream {
                            Ok(mut s) => {
                                let _ = s.set_read_timeout(Some(Duration::from_millis(2000)));
                                let _ = s.set_write_timeout(Some(Duration::from_millis(2000)));
                                let stream_clone = match s.try_clone() {
                                    Ok(c) => c,
                                    Err(_) => continue,
                                };
                                let mut reader = BufReader::new(stream_clone);
                                let mut line = String::new();
                                if reader.read_line(&mut line).is_ok() {
                                    let cmd = line.trim();
                                    match cmd {
                                        "PING" => {
                                            let _ = writeln!(s, "PONG");
                                            let _ = s.flush();
                                        }
                                        "SHOW" => {
                                            on_action("SHOW");
                                            let _ = writeln!(s, "OK");
                                            let _ = s.flush();
                                        }
                                        "RESCAN" => {
                                            on_action("RESCAN");
                                            let _ = writeln!(s, "OK");
                                            let _ = s.flush();
                                        }
                                        other => {
                                            on_action(other);
                                            let _ = writeln!(s, "OK");
                                            let _ = s.flush();
                                        }
                                    }
                                }
                            }
                            Err(_) => break,
                        }
                    }
                }
            })
            .expect("Failed to spawn single-instance IPC listener thread");

        handle
    }
}

pub enum InstanceStatus {
    AlreadyRunning(SingleInstanceClient),
    Primary(SingleInstanceServer),
}

/// Attempts to connect to an existing running instance for this index path.
pub fn connect_client(index_path: &str) -> Result<SingleInstanceClient, Box<dyn std::error::Error + Send + Sync>> {
    let endpoint = get_endpoint_path(index_path);
    let mut client = SingleInstanceClient::new(endpoint);
    let reply = client.send_action("PING")?;
    if reply == "PONG" {
        Ok(client)
    } else {
        Err("Endpoint did not respond with PONG".into())
    }
}

/// Checks whether an instance is already running. If so, returns `AlreadyRunning(client)`.
/// If not, binds as `Primary(server)`. Stale socket files and racing processes are handled safely.
pub fn check_or_bind(index_path: &str) -> Result<InstanceStatus, Box<dyn std::error::Error + Send + Sync>> {
    let endpoint = get_endpoint_path(index_path);

    // 1. Try to connect to existing instance
    if let Ok(client) = connect_client(index_path) {
        return Ok(InstanceStatus::AlreadyRunning(client));
    }

    // 2. If connect failed, any existing socket file is stale from a prior terminated process.
    if endpoint.exists() {
        eprintln!("[SingleInstance] Cleaning up stale endpoint socket at {:?}", endpoint);
        let _ = std::fs::remove_file(&endpoint);
    }

    // Ensure parent directory exists
    if let Some(parent) = endpoint.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    // 3. Bind as primary instance
    #[cfg(unix)]
    {
        let listener = match UnixListener::bind(&endpoint) {
            Ok(l) => l,
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                // Raced with another process; wait and retry connecting once
                std::thread::sleep(Duration::from_millis(100));
                if let Ok(client) = connect_client(index_path) {
                    return Ok(InstanceStatus::AlreadyRunning(client));
                }
                return Err(Box::new(e));
            }
            Err(e) => return Err(Box::new(e)),
        };

        Ok(InstanceStatus::Primary(SingleInstanceServer {
            endpoint,
            listener,
        }))
    }
    #[cfg(not(unix))]
    {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        std::fs::write(&endpoint, port.to_string())?;

        Ok(InstanceStatus::Primary(SingleInstanceServer {
            endpoint,
            listener,
        }))
    }
}
