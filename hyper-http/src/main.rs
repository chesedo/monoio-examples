use bytes::Bytes;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::header::CONTENT_TYPE;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use std::convert::Infallible;
use std::error::Error;
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Define the socket address
    let addr: SocketAddr = ([127, 0, 0, 1], 8080).into();

    // Create a TCP listener
    let listener = TcpListener::bind(addr).await?;
    println!("Listening on http://{addr}");

    // Accept connections and process them
    loop {
        let (tcp_stream, _) = listener.accept().await?;
        tokio::task::spawn(handle_connection(tcp_stream));
    }
}

async fn handle_connection(stream: TcpStream) -> Result<(), Box<dyn Error + Send>> {
    let io = TokioIo::new(stream);

    http1::Builder::new()
        .serve_connection(io, service_fn(handle_request))
        .await
        .unwrap();

    Ok(())
}

// Handle the HTTP request
async fn handle_request(req: Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    // Create a response based on the path
    let res = match req.uri().path() {
        "/" => Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, "text/plain")
            .body(Full::new(Bytes::from("Hello, World!")))
            .unwrap(),
        "/health" => Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, "text/plain")
            .body(Full::new(Bytes::from("OK")))
            .unwrap(),
        _ => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(CONTENT_TYPE, "text/plain")
            .body(Full::new(Bytes::from("Not Found")))
            .unwrap(),
    };

    Ok(res)
}
