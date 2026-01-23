mod api;

use std::{
    error::Error,
    net::{Ipv4Addr, SocketAddrV4},
    process::ExitCode,
    usize,
};

use httparse::{EMPTY_HEADER, Request};
use smol::{
    channel::{Receiver, Sender},
    io::AsyncReadExt,
    net::{TcpListener, TcpStream},
    stream::StreamExt,
};

use crate::api::get_hello_world;

const ADDR: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 3333);

fn main() -> ExitCode {
    let Ok(socket) = smol::block_on(TcpListener::bind(ADDR)) else {
        println!("Can not bind to {}", ADDR);
        return ExitCode::FAILURE;
    };

    let (sender, receiver) = smol::channel::unbounded();

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
    if let Err(e) = smol::block_on(sender.send(stream)) {
        println!("{e}; connection dropped")
    };
}

#[inline(always)]
pub async fn cycle_executor(executor: &smol::Executor<'_>, receiver: &Receiver<TcpStream>) {
    println!("CYLCE");
    async fn error_handler<T, F: Future<Output = Result<T, Box<dyn Error>>>>(r: F) {
        if let Err(e) = r.await {
            println!("{e}");
        }
    }

    let get_msg = || {
        if executor.is_empty() {
            receiver.recv_blocking().ok()
        } else {
            receiver.try_recv().ok()
        }
    };

    while let Some(stream) = get_msg() {
        executor
            .spawn(error_handler(handle_stream(stream)))
            .detach();
        println!("NEW TASK");
    }

    while executor.try_tick() {
        println!("TICK")
    }
}

fn handler_thread(receiver: Receiver<TcpStream>) {
    let executor = smol::Executor::new();
    std::thread::scope(|scope| {
        (0..4).into_iter().for_each(|_| {
            let receiver = receiver.clone();
            let executor = &executor;
            scope.spawn(move || {
                loop {
                    smol::block_on(cycle_executor(executor, &receiver))
                }
            });
        });
    });
}

pub async fn handle_stream(mut stream: TcpStream) -> Result<(), Box<dyn Error>> {
    let (buff, header_count) = read_header(&mut stream).await?;

    let headers = &mut *vec![EMPTY_HEADER; header_count].into_boxed_slice();

    let request = &mut Request::new(headers);
    request.parse(buff.as_slice())?;

    get_hello_world(request, &mut stream).await;

    println!("{:?}", request);
    Ok(())
}

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
