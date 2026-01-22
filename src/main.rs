use std::{
    error::Error,
    net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream},
    pin::Pin,
    process::ExitCode,
    sync::mpsc::{Receiver, Sender},
};

use httparse::{self, EMPTY_HEADER, Header, Request, Response};
use smol::{
    Async,
    io::{AsyncReadExt, Bytes},
    stream::StreamExt,
};

const ADDR: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 3333);

fn main() -> ExitCode {
    let Ok(socket) = TcpListener::bind(ADDR) else {
        println!("Can not bind to {}", ADDR);
        return ExitCode::FAILURE;
    };

    let executor = smol::Executor::new();
    let (sender, receiver) = std::sync::mpsc::channel();

    let _ = std::thread::spawn(move || {
        loop {
            smol::block_on(cycle_executor(&executor, &receiver));
        }
    });

    for stream in socket.incoming() {
        match stream {
            Ok(s) => add_to_queue(s, &sender),
            Err(e) => println!("encountered IO error: {e}"),
        }
    }

    return ExitCode::SUCCESS;
}

#[inline(always)]
pub fn add_to_queue(stream: TcpStream, sender: &Sender<Async<TcpStream>>) {
    let Ok(stream) = Async::new(stream) else {
        println!("Could not upgrade stream to async, dropped connection");
        return;
    };

    if let Err(e) = sender.send(stream) {
        println!("{e}; connection dropped")
    };
}

#[inline(always)]
pub async fn cycle_executor(executor: &smol::Executor<'_>, receiver: &Receiver<Async<TcpStream>>) {
    async fn error_handler<T, F: Future<Output = Result<T, Box<dyn Error>>>>(r: F) {
        if let Err(e) = r.await {
            println!("{e}");
        }
    }

    for task in receiver.try_iter() {
        executor.spawn(error_handler(handle_stream(task))).detach();
    }

    while executor.try_tick() {}
}

pub async fn handle_stream(stream: Async<TcpStream>) -> Result<(), Box<dyn Error>> {
    let bytes = &mut stream.bytes();
    let buff = parse_path(bytes).await?;
    let headers = &mut [EMPTY_HEADER; 0];

    let mut request = Request::new(headers);
    request.parse(buff.as_slice())?;

    let path = match request.path {
        Some(p) => p,
        None => {
            println!("malformed request");
            return Ok(());
        }
    };

    println!("{path}");
    Ok(())
}

async fn parse_path(bytes: &mut Bytes<Async<TcpStream>>) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut buff = Vec::with_capacity(500);
    while let Some(byte) = bytes.next().await {
        buff.push(byte?);

        if buff.len() < 2 {
            continue;
        }

        if buff[buff.len() - 2..] == *b"\r\n" {
            break;
        }
    }
    Ok(buff)
}
