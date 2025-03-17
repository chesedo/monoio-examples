use bytes::{BufMut, BytesMut};
use http::header::{CONTENT_LENGTH, CONTENT_TYPE, DATE};
use http::{Response, StatusCode};
use httparse::{EMPTY_HEADER, Status};
use httpdate::fmt_http_date;
use monoio::io::{AsyncReadRent, AsyncWriteRentExt};
use monoio::net::{TcpListener, TcpStream};
use std::error::Error;
use std::net::SocketAddr;
use std::time::SystemTime;

const MAX_HEADERS: usize = 64;
const BUFFER_SIZE: usize = 8192;

#[monoio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Define the socket address
    let addr: SocketAddr = ([127, 0, 0, 1], 8080).into();

    // Create a TCP listener
    let listener = TcpListener::bind(addr)?;
    println!("Listening on http://{addr}");

    // Accept connections and process them
    loop {
        let (stream, _) = listener.accept().await?;
        monoio::spawn(handle_connection(stream));
    }
}

async fn handle_connection(mut stream: TcpStream) -> Result<(), Box<dyn Error>> {
    let mut buffer = BytesMut::with_capacity(BUFFER_SIZE);

    // Keep reading from the connection until it's closed
    loop {
        // Read the request
        buffer.clear();
        let (res, buf) = stream.read(buffer).await;
        buffer = buf;

        match res {
            Ok(0) => return Ok(()), // Connection closed
            Ok(bytes_read) => {
                // Process the HTTP request
                let mut headers = [EMPTY_HEADER; MAX_HEADERS];
                let mut req = httparse::Request::new(&mut headers);

                match req.parse(&buffer[..bytes_read]) {
                    Ok(Status::Complete(_)) => {
                        // Create and send response
                        let res = handle_request(req).await?;

                        // Serialize response
                        let response_bytes = serialize_response(&res);

                        // Write response
                        if let Err(e) = stream.write_all(response_bytes).await.0 {
                            return Err(e.into());
                        }
                    }
                    Ok(Status::Partial) => {
                        // Handle incomplete request - in a real server you might wait for more data
                        // For simplicity in benchmark, treat as error
                        let response = Response::builder()
                            .status(StatusCode::BAD_REQUEST)
                            .header(CONTENT_TYPE, "text/plain")
                            .body("Incomplete HTTP request")?;

                        let response_bytes = serialize_response(&response);
                        stream.write_all(response_bytes).await.0?;
                        return Ok(());
                    }
                    Err(_) => {
                        // Handle parsing error
                        let response = Response::builder()
                            .status(StatusCode::BAD_REQUEST)
                            .header(CONTENT_TYPE, "text/plain")
                            .body("Bad Request")?;

                        let response_bytes = serialize_response(&response);
                        stream.write_all(response_bytes).await.0?;
                        return Ok(());
                    }
                }
            }
            Err(e) => return Err(e.into()),
        }
    }
}

async fn handle_request<'a>(
    req: httparse::Request<'a, 'a>,
) -> Result<Response<&'a str>, Box<dyn Error>> {
    // Create a response based on the path
    let res = match req.path.expect("the request is complete") {
        "/" => Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, "text/plain")
            .body("Hello, World!")?,
        "/health" => Response::builder()
            .status(StatusCode::OK)
            .header(CONTENT_TYPE, "text/plain")
            .body("Ok")?,
        _ => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(CONTENT_TYPE, "text/plain")
            .body("Not Found")?,
    };

    Ok(res)
}

// Optimized response serializer that returns bytes directly
fn serialize_response<T: AsRef<[u8]>>(response: &Response<T>) -> BytesMut {
    let status = response.status();
    let headers = response.headers();
    let body = response.body().as_ref();

    // Pre-allocate a reasonable buffer size
    // Status line + headers + body + some extra space
    let capacity = 128 + (headers.len() * 32) + body.len();
    let mut buffer = BytesMut::with_capacity(capacity);

    // Write status line
    buffer.put_slice(b"HTTP/1.1 ");
    buffer.put_slice(status.as_u16().to_string().as_bytes());
    buffer.put_slice(b" ");
    buffer.put_slice(status.canonical_reason().unwrap_or("").as_bytes());
    buffer.put_slice(b"\r\n");

    // Add headers
    for (name, value) in headers.iter() {
        buffer.put_slice(name.as_str().as_bytes());
        buffer.put_slice(b": ");
        buffer.put_slice(value.as_bytes());
        buffer.put_slice(b"\r\n");
    }

    // Add Content-Length if not present
    if !headers.contains_key(CONTENT_LENGTH) {
        buffer.put_slice(CONTENT_LENGTH.as_str().as_bytes());
        buffer.put_slice(b": ");
        buffer.put_slice(body.len().to_string().as_bytes());
        buffer.put_slice(b"\r\n");
    }

    // Add Date if not present
    if !headers.contains_key(DATE) {
        let now = SystemTime::now();
        buffer.put_slice(DATE.as_str().as_bytes());
        buffer.put_slice(b": ");
        buffer.put_slice(fmt_http_date(now).as_bytes());
        buffer.put_slice(b"\r\n");
    }

    // Finish headers
    buffer.put_slice(b"\r\n");

    // Add body
    buffer.put_slice(body);

    buffer
}
