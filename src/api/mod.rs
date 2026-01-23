use httparse::Request;
use smol::{io::AsyncWriteExt, net::TcpStream};

pub async fn get_hello_world(_request: &mut Request<'_, '_>, stream: &mut TcpStream) {
    if let Err(err) = stream.write_all(b"Hello World!").await {
        println!("{err}");
    };
}

// pub fn get_echo() {}
