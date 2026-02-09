use std::pin::Pin;

use smol::{future::FutureExt, io::AsyncWriteExt, net::TcpStream};

use crate::OwnedRequest;

pub type Endpoint = &'static dyn Fn(OwnedRequest, TcpStream) -> Pin<Box<EndpointFuture>>;

pub const GET_HELLO_WORLD: Endpoint = &|request: OwnedRequest, stream: TcpStream| {
    pub async fn get_hello_world(mut _request: OwnedRequest, mut stream: TcpStream) {
        if let Err(err) = stream.write_all(b"Hello World!").await {
            println!("{err}");
        };
    }
    EndpointFuture::new(move || Box::pin(get_hello_world(request, stream)))
};

pub struct EndpointFuture {
    inner: Pin<Box<dyn Future<Output = ()>>>,
}

impl EndpointFuture {
    pub fn new<T: FnOnce() -> Pin<Box<dyn Future<Output = ()>>>>(f: T) -> Pin<Box<Self>> {
        Box::pin(EndpointFuture { inner: f() })
    }
}

unsafe impl Send for EndpointFuture {}

impl Future for EndpointFuture {
    type Output = ();

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        self.inner.poll(cx)
    }
}
