mod api;

use std::{
    error::Error,
    net::{Ipv4Addr, SocketAddrV4},
    process::ExitCode,
    sync::mpsc::{Receiver, Sender},
    usize,
};

use httparse::{EMPTY_HEADER, Request};
use smol::{
    io::AsyncReadExt,
    net::{TcpListener, TcpStream},
    stream::StreamExt,
};

const ADDR: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 3333);

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
    println!("INCOMING");
    if let Err(e) = sender.send(stream) {
        println!("{e}; connection dropped")
    };
}

#[inline(always)]
fn handler_thread(receiver: Receiver<TcpStream>) {
    async fn task_wrapper(mut stream: TcpStream) {
        let result = handle_stream(&mut stream).await;

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
async fn read_header(stream: &mut TcpStream) -> Result<(Vec<u8>, usize), Box<dyn Error>> {
    let bytes = &mut stream.bytes();
    let mut header_count = usize::MAX - 1; //adjust for overcounting
    let mut buff = Vec::with_capacity(4000);

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
    buff.shrink_to_fit();
    Ok((buff, header_count))
}

#[inline(always)]
pub async fn handle_stream(stream: &mut TcpStream) -> Result<(), Box<dyn Error>> {
    let (buff, header_count) = read_header(stream).await?;

    let headers = &mut *vec![EMPTY_HEADER; header_count].into_boxed_slice();

    let request = &mut Request::new(headers);
    request.parse(buff.as_slice())?;

    println!("{:?}", request);
    Ok(())
}
