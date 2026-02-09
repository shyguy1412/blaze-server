mod api;
mod router;

use std::{
    net::{Ipv4Addr, SocketAddrV4},
    ops::{Deref, DerefMut},
    process::ExitCode,
    sync::mpsc::{Receiver, Sender},
    usize,
};

use httparse::{EMPTY_HEADER, Header, Request};
use smol::{
    io::AsyncReadExt,
    net::{TcpListener, TcpStream},
    stream::StreamExt,
};
use url::Url;

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

const ADDR: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 3333);

#[derive(Clone, Copy, Debug)]
enum Error {
    MalformedRequest,
    InvalidMethod,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Copy, Debug)]
enum HttpMethod {
    GET,
    PUT,
    POST,
    DELETE,
    OPTION,
    TRACE,
}

impl TryFrom<&String> for HttpMethod {
    fn try_from(value: &String) -> std::result::Result<HttpMethod, Self::Error> {
        use HttpMethod::*;
        match value.to_ascii_uppercase().as_str() {
            "GET" => Ok(GET),
            "PUT" => Ok(PUT),
            "POST" => Ok(POST),
            "DELETE" => Ok(DELETE),
            "OPTION" => Ok(OPTION),
            "TRACE" => Ok(TRACE),
            _ => Err(Error::InvalidMethod),
        }
    }

    type Error = Error;
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

fn main() -> ExitCode {
    let Ok(socket) = smol::block_on(TcpListener::bind(ADDR)) else {
        println!("Can not bind to {}", ADDR);
        return ExitCode::FAILURE;
    };

    let (sender, receiver) = std::sync::mpsc::channel();

    let _ = std::thread::spawn(|| handler_thread(receiver));

    let incoming = &mut socket.incoming();

    while let Some(stream) = smol::block_on(incoming.next()) {
        match stream {
            Ok(s) => add_to_queue(s, &sender),
            Err(e) => println!("encountered IO error: {e}"),
        }
    }

    return ExitCode::SUCCESS;
}

#[inline(always)]
pub fn add_to_queue(stream: TcpStream, sender: &Sender<TcpStream>) {
    if let Err(e) = sender.send(stream) {
        println!("{e}; connection dropped")
    };
}

#[inline(always)]
fn handler_thread(receiver: Receiver<TcpStream>) {
    async fn task_wrapper(stream: TcpStream) {
        let result = handle_stream(stream).await;

        if let Err(e) = result {
            println!("{e}");
        }
    }

    let executor = &smol::Executor::new();

    let scheduler_thread = move || {
        receiver.iter().for_each(|stream| {
            let task = task_wrapper(stream);
            executor.spawn(task).detach();
        })
    };

    let task_runner_thread = move || {
        loop {
            smol::block_on(executor.tick())
        }
    };

    std::thread::scope(move |scope| {
        scope.spawn(scheduler_thread);

        std::thread::available_parallelism()
            .map(|n| 0..n.into())
            .unwrap_or(0..1)
            .for_each(|_| {
                scope.spawn(task_runner_thread);
            });
    });
}

#[inline(always)]
async fn read_first_line(stream: &mut TcpStream, buff: &mut Vec<u8>) -> Result<()> {
    let bytes = &mut stream.bytes();

    while let Some(byte) = bytes.next().await {
        buff.push(byte?);

        if buff.len() < 2 {
            continue;
        }

        if buff[buff.len() - 2..] == *b"\r\n" {
            break;
        }
    }
    Ok(())
}

#[inline(always)]
async fn read_header(stream: &mut TcpStream, buff: &mut Vec<u8>) -> Result<usize> {
    let bytes = &mut stream.bytes();
    let mut header_count = usize::MAX; //adjust for overcounting

    while let Some(byte) = bytes.next().await {
        buff.push(byte?);

        if buff.len() < 4 {
            continue;
        }

        if buff[buff.len() - 2..] == *b"\r\n" {
            header_count = header_count.wrapping_add(1);
        }

        if buff[buff.len() - 4..] == *b"\r\n\r\n" {
            break;
        }
    }
    Ok(header_count)
}

#[inline(always)]
pub async fn handle_stream(mut stream: TcpStream) -> Result<()> {
    let mut buffer = Vec::with_capacity(4000);

    read_first_line(&mut stream, &mut buffer).await?;

    let [method, path, http_version]: &[String] = &buffer[0..buffer.len() - 2]
        .split(|a| *a as char == ' ')
        .filter_map(|chunk| str::from_utf8(chunk).ok())
        .map(|str| str.to_string())
        .collect::<Vec<_>>()
    else {
        return Err(Error::MalformedRequest)?;
    };

    let url = Url::parse(&format!("http://local{path}"))?;
    let method: HttpMethod = method.try_into()?;

    let header_count = read_header(&mut stream, &mut buffer).await?;
    let endpoint = router::route(url).await?;

    let request = OwnedRequest::new(buffer, header_count).ok_or(Error::MalformedRequest)?;

    endpoint(request, stream).await;

    println!("Method: {method}; Path: {path}; HTTP Version: {http_version}");

    Ok(())
}

struct OwnedRequest {
    buffer: *mut Vec<u8>,
    headers: *mut [Header<'static>],
    request: Request<'static, 'static>,
}

impl Deref for OwnedRequest {
    type Target = Request<'static, 'static>;

    fn deref(&self) -> &Self::Target {
        &self.request
    }
}

impl DerefMut for OwnedRequest {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.request
    }
}

impl OwnedRequest {
    pub fn new(buffer: Vec<u8>, header_count: usize) -> Option<Self> {
        let headers = Box::into_raw(vec![EMPTY_HEADER; header_count].into_boxed_slice());
        let buffer = Box::into_raw(Box::new(buffer));
        let mut request: Request<'static, 'static> =
            Request::new(unsafe { headers.as_mut().expect("It as literally just created") });

        request
            .parse(unsafe { buffer.as_ref().expect("Was just created") })
            .inspect_err(|e| println!("{e:?}, {header_count}"))
            .ok()?;

        Some(OwnedRequest {
            buffer,
            headers,
            request,
        })
    }
}

impl Drop for OwnedRequest {
    fn drop(&mut self) {
        unsafe {
            drop(Box::from_raw(self.buffer));
            drop(Box::from_raw(self.headers))
        }
    }
}
