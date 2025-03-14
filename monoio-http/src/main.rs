use bytes::BytesMut;
use http::header::CONTENT_TYPE;
use http::{Response, StatusCode};
use httparse::{EMPTY_HEADER, Status};
use monoio::io::{AsyncReadRent, AsyncWriteRentExt};
use monoio::net::{TcpListener, TcpStream};
use std::error::Error;
use std::net::SocketAddr;

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
                        let serialized = serialize_response(&res);

                        // Write response
                        if let Err(e) = stream.write_all(serialized.into_bytes()).await.0 {
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

                        let serialized = serialize_response(&response);
                        stream.write_all(serialized.into_bytes()).await.0?;
                        return Ok(());
                    }
                    Err(_) => {
                        // Handle parsing error
                        let response = Response::builder()
                            .status(StatusCode::BAD_REQUEST)
                            .header(CONTENT_TYPE, "text/plain")
                            .body("Bad Request")?;

                        let serialized = serialize_response(&response);
                        stream.write_all(serialized.into_bytes()).await.0?;
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

// Simple but effective response serializer
fn serialize_response<T: AsRef<[u8]>>(response: &Response<T>) -> String {
    let status = response.status();
    let headers = response.headers();
    let body = response.body().as_ref();

    let mut result = format!(
        "HTTP/1.1 {} {}\r\n",
        status.as_u16(),
        status.canonical_reason().unwrap_or("")
    );

    // Add headers
    for (name, value) in headers.iter() {
        if let Ok(value_str) = value.to_str() {
            result.push_str(&format!("{}: {}\r\n", name, value_str));
        }
    }

    // Add Content-Length
    if !headers.contains_key("Content-Length") {
        result.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }

    // Finish headers and add body
    result.push_str("\r\n");
    result.push_str(&String::from_utf8_lossy(body));

    result
}
