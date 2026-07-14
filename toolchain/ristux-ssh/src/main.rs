use std::env;
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use russh::client;
use russh::keys::{PrivateKeyWithHashAlg, load_secret_key};
use russh::{ChannelMsg, Disconnect};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, ReadBuf};

type AnyError = Box<dyn std::error::Error + Send + Sync>;

#[cfg(target_os = "ristux")]
#[unsafe(no_mangle)]
unsafe extern "Rust" fn __getrandom_v03_custom(
    dest: *mut u8,
    len: usize,
) -> Result<(), getrandom::Error> {
    // Ristux exposes its kernel entropy pool through /dev/urandom. Initialize
    // the memory before constructing the slice as required by getrandom's
    // custom-backend contract.
    unsafe { core::ptr::write_bytes(dest, 0, len) };
    let bytes = unsafe { core::slice::from_raw_parts_mut(dest, len) };
    std::fs::File::open("/dev/urandom")
        .and_then(|mut random| random.read_exact(bytes))
        .map_err(|_| getrandom::Error::UNSUPPORTED)
}

#[cfg(target_os = "ristux")]
#[unsafe(no_mangle)]
extern "C" fn log2(value: f64) -> f64 {
    libm::log2(value)
}

#[cfg(target_os = "ristux")]
#[unsafe(no_mangle)]
extern "C" fn pow(value: f64, exponent: f64) -> f64 {
    libm::pow(value, exponent)
}

#[derive(Debug)]
struct Options {
    host: String,
    user: String,
    port: u16,
    key: Option<PathBuf>,
    strict_host_key: bool,
    verbose: bool,
    command: Vec<String>,
}

impl Options {
    fn parse() -> Result<Self, AnyError> {
        let mut args = env::args().skip(1).peekable();
        let mut port = 22;
        let mut user = None;
        let mut key = None;
        let mut strict_host_key = true;
        let mut verbose = false;
        let mut host = None;
        let mut command = Vec::new();

        while let Some(arg) = args.next() {
            if host.is_some() {
                command.push(arg);
                command.extend(&mut args);
                break;
            }
            match arg.as_str() {
                "-p" => port = args.next().ok_or("-p requires a port")?.parse()?,
                "-l" => user = Some(args.next().ok_or("-l requires a user")?),
                "-i" => key = Some(PathBuf::from(args.next().ok_or("-i requires a path")?)),
                "-o" => {
                    let option = args.next().ok_or("-o requires an option")?;
                    if matches!(
                        option.as_str(),
                        "StrictHostKeyChecking=no" | "StrictHostKeyChecking=off"
                    ) {
                        strict_host_key = false;
                    }
                }
                "-v" | "-vv" | "-vvv" => verbose = true,
                "-4" | "-6" | "-T" | "-x" | "-q" => {}
                "--" => host = args.next(),
                _ if arg.starts_with('-') => {
                    return Err(format!("unsupported ssh option: {arg}").into());
                }
                _ => host = Some(arg),
            }
        }

        let mut host = host.ok_or("usage: ssh [-p port] [-l user] [-i key] host command")?;
        if command.is_empty() {
            command.extend(args);
        }
        if command.is_empty() {
            return Err("ristux-ssh requires a remote command".into());
        }
        if let Some(separator) = host.find('@') {
            let from_host = host[..separator].to_owned();
            let hostname = host[separator + 1..].to_owned();
            if user.is_none() {
                user = Some(from_host);
            }
            host = hostname;
        }
        let user = user
            .or_else(|| env::var("USER").ok())
            .unwrap_or_else(|| "git".to_owned());
        let key = key.or_else(default_private_key);
        Ok(Self {
            host,
            user,
            port,
            key,
            strict_host_key,
            verbose,
            command,
        })
    }
}

fn default_private_key() -> Option<PathBuf> {
    let home = env::var_os("HOME")?;
    ["id_ed25519", "id_ecdsa"]
        .into_iter()
        .map(|name| PathBuf::from(&home).join(".ssh").join(name))
        .find(|path| path.is_file())
}

struct Handler {
    host: String,
    port: u16,
    strict: bool,
}

impl client::Handler for Handler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        if !self.strict {
            return Ok(true);
        }
        russh::keys::known_hosts::check_known_hosts(
            &self.host,
            self.port,
            server_public_key,
        )
        .map_err(russh::Error::from)
    }
}

struct ReadState {
    chunks: VecDeque<Vec<u8>>,
    offset: usize,
    eof: bool,
    error: Option<(std::io::ErrorKind, String)>,
    waker: Option<std::task::Waker>,
}

struct ThreadedStream {
    writer: TcpStream,
    read: Arc<Mutex<ReadState>>,
}

impl ThreadedStream {
    fn new(socket: TcpStream) -> std::io::Result<Self> {
        let mut reader = socket.try_clone()?;
        let read = Arc::new(Mutex::new(ReadState {
            chunks: VecDeque::new(),
            offset: 0,
            eof: false,
            error: None,
            waker: None,
        }));
        let state = read.clone();
        std::thread::Builder::new()
            .name("ssh-socket-read".to_owned())
            .spawn(move || {
                let mut buffer = [0u8; 16 * 1024];
                loop {
                    let result = reader.read(&mut buffer);
                    let mut state = state.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                    match result {
                        Ok(0) => state.eof = true,
                        Ok(count) => state.chunks.push_back(buffer[..count].to_vec()),
                        Err(error) => {
                            state.error = Some((error.kind(), error.to_string()));
                        }
                    }
                    if let Some(waker) = state.waker.take() {
                        waker.wake();
                    }
                    if state.eof || state.error.is_some() {
                        break;
                    }
                }
            })?;
        Ok(Self {
            writer: socket,
            read,
        })
    }
}

impl AsyncRead for ThreadedStream {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let mut state = self
            .read
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(chunk) = state.chunks.pop_front() {
            let available = &chunk[state.offset..];
            let count = available.len().min(buffer.remaining());
            buffer.put_slice(&available[..count]);
            if count < available.len() {
                state.offset += count;
                state.chunks.push_front(chunk);
            } else {
                state.offset = 0;
            }
            return Poll::Ready(Ok(()));
        }
        if let Some((kind, message)) = state.error.take() {
            return Poll::Ready(Err(std::io::Error::new(kind, message)));
        }
        if state.eof {
            return Poll::Ready(Ok(()));
        }
        state.waker = Some(context.waker().clone());
        Poll::Pending
    }
}

impl AsyncWrite for ThreadedStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Poll::Ready(self.writer.write(buffer))
    }

    fn poll_flush(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(self.writer.flush())
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(self.writer.shutdown(std::net::Shutdown::Write))
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("ssh: {error}");
        std::process::exit(255);
    }
}

fn run() -> Result<(), AnyError> {
    let options = Options::parse()?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_time()
        .build()?;
    runtime.block_on(run_async(options))
}

async fn run_async(options: Options) -> Result<(), AnyError> {
    if options.verbose {
        eprintln!("ssh: connecting to {}:{}", options.host, options.port);
    }
    let socket = TcpStream::connect((options.host.as_str(), options.port))?;
    socket.set_nodelay(true)?;
    if options.verbose {
        eprintln!("ssh: TCP connection established");
    }
    let config = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(30)),
        nodelay: true,
        ..Default::default()
    });
    let handler = Handler {
        host: options.host.clone(),
        port: options.port,
        strict: options.strict_host_key,
    };
    let mut session = client::connect_stream(config, ThreadedStream::new(socket)?, handler).await?;
    if options.verbose {
        eprintln!("ssh: SSH key exchange complete");
    }

    let authenticated = if let Some(key_path) = options.key {
        let key = load_secret_key(key_path, None)?;
        session
            .authenticate_publickey(
                options.user.clone(),
                PrivateKeyWithHashAlg::new(
                    Arc::new(key),
                    session.best_supported_rsa_hash().await?.flatten(),
                ),
            )
            .await?
            .success()
    } else {
        session.authenticate_none(options.user.clone()).await?.success()
    };
    if !authenticated {
        return Err("authentication failed (provide an Ed25519 key with -i)".into());
    }
    if options.verbose {
        eprintln!("ssh: authenticated as {}", options.user);
    }

    let mut channel = session.channel_open_session().await?;
    channel.exec(true, options.command.join(" ")).await?;
    if options.verbose {
        eprintln!("ssh: remote command started");
    }
    let mut writer = channel.make_writer();
    let runtime_handle = tokio::runtime::Handle::current();
    let input = std::thread::spawn(move || -> Result<(), AnyError> {
        let mut stdin = std::io::stdin().lock();
        let mut buffer = [0u8; 16 * 1024];
        loop {
            let read = stdin.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            runtime_handle.block_on(writer.write_all(&buffer[..read]))?;
        }
        runtime_handle.block_on(writer.shutdown())?;
        Ok(())
    });

    let mut exit_status = None;
    while let Some(message) = channel.wait().await {
        match message {
            ChannelMsg::Data { data } => {
                std::io::stdout().write_all(&data)?;
                std::io::stdout().flush()?;
            }
            ChannelMsg::ExtendedData { data, .. } => {
                std::io::stderr().write_all(&data)?;
                std::io::stderr().flush()?;
            }
            ChannelMsg::ExitStatus { exit_status: status } => exit_status = Some(status),
            _ => {}
        }
    }
    input.join().map_err(|_| "stdin forwarding thread panicked")??;
    session
        .disconnect(Disconnect::ByApplication, "", "English")
        .await?;
    match exit_status.unwrap_or(0) {
        0 => Ok(()),
        status => Err(format!("remote command exited with status {status}").into()),
    }
}
