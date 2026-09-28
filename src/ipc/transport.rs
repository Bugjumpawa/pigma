//! Platform transport for the IPC endpoint: a Unix domain socket on
//! Linux/macOS and a named pipe on Windows.
//!
//! This is the only IPC module that branches on `cfg(unix)` / `cfg(windows)`
//! for the endpoint itself; [`server`](super::server) and [`client`](super::client)
//! speak in terms of the [`Listener`], [`Stream`] and [`ClientStream`] types
//! defined here.

use super::path::resolve_socket_path;

/// The stream accepted by the server (Unix socket on unix, named pipe on
/// Windows).
#[cfg(unix)]
pub(super) type Stream = tokio::net::UnixStream;
#[cfg(windows)]
pub(super) type Stream = tokio::net::windows::named_pipe::NamedPipeServer;

/// The stream a client connects with (Unix socket on unix, named pipe on
/// Windows).
#[cfg(unix)]
pub(super) type ClientStream = tokio::net::UnixStream;
#[cfg(windows)]
pub(super) type ClientStream = tokio::net::windows::named_pipe::NamedPipeClient;

/// Connect to the running instance's listener endpoint.
pub(super) async fn connect_client(path: &std::path::Path) -> std::io::Result<ClientStream> {
    #[cfg(unix)]
    {
        ClientStream::connect(path).await
    }
    #[cfg(windows)]
    {
        tokio::net::windows::named_pipe::ClientOptions::new().open(path.to_string_lossy().as_ref())
    }
}

/// Platform-specific listener for the IPC server.
///
/// On Windows a named pipe is re-created for every connection, so the Windows
/// variant holds the pipe name rather than a persistent handle.
pub(super) struct Listener {
    #[cfg(unix)]
    unix: tokio::net::UnixListener,
    #[cfg(windows)]
    name: String,
}

impl Listener {
    /// Bind the listener, clearing any stale file left by a previous run on
    /// Unix. Returns `None` when another pigma instance already holds the
    /// endpoint.
    pub(super) fn bind() -> Option<Self> {
        let path = resolve_socket_path();
        #[cfg(unix)]
        {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            match tokio::net::UnixListener::bind(&path) {
                Ok(unix) => Some(Self { unix }),
                Err(_) => {
                    // Either a live instance owns the socket or it is stale.
                    // A connect probe tells us which: if we can connect, another
                    // instance is running and we must not steal the socket.
                    if std::os::unix::net::UnixStream::connect(&path).is_ok() {
                        log::warn!(
                            "ipc: another pigma instance already owns {}",
                            path.display()
                        );
                        return None;
                    }
                    let _ = std::fs::remove_file(&path);
                    tokio::net::UnixListener::bind(&path)
                        .ok()
                        .map(|unix| Self { unix })
                }
            }
        }
        #[cfg(windows)]
        {
            let name = path.to_string_lossy().into_owned();
            match tokio::net::windows::named_pipe::ServerOptions::new().create(&name) {
                Ok(_) => Some(Self { name }),
                Err(e) => {
                    // Windows releases the pipe name when the owning process
                    // exits, so a failed bind always means a live instance.
                    log::warn!("ipc: another pigma instance already owns the pipe {name}: {e}");
                    None
                }
            }
        }
    }

    /// Wait for the next incoming connection, returning the accepted stream.
    pub(super) async fn next(&mut self) -> Option<Stream> {
        #[cfg(unix)]
        {
            self.unix.accept().await.ok().map(|(stream, _)| stream)
        }
        #[cfg(windows)]
        {
            // A fresh server instance per connection; after a client attaches,
            // that instance becomes the connection stream.
            let server = tokio::net::windows::named_pipe::ServerOptions::new()
                .create(&self.name)
                .ok()?;
            server.connect().await.ok()?;
            Some(server)
        }
    }
}

/// Remove the endpoint file so a later run can rebind it. No-op on Windows,
/// which releases the pipe name when the process exits.
pub(super) fn remove_endpoint() {
    #[cfg(unix)]
    let _ = std::fs::remove_file(resolve_socket_path());
}
