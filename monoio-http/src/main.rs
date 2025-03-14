use bytes::BytesMut;
use http::header::CONTENT_TYPE;
use http::{Response, StatusCode};
use httparse::Status;
use monoio::io::{AsyncReadRent, AsyncWriteRentExt};
use monoio::net::{TcpListener, TcpStream};
use std::error::Error;
use std::net::SocketAddr;

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
    let buffer = BytesMut::with_capacity(4096);

    // Read the request
    let (res, buffer) = stream.read(buffer).await;
    let bytes_read = res?;

    if bytes_read == 0 {
        return Ok(()); // Connection closed
    }

    // Parse the HTTP request using httparse
    let mut headers = [httparse::EMPTY_HEADER; 16];
    let mut req = httparse::Request::new(&mut headers);

    let response = match req.parse(&buffer) {
        Ok(Status::Complete(_)) => handle_request(req).await?,
        Ok(Status::Partial) => {
            // Incomplete request
            Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header(CONTENT_TYPE, "text/plain")
                .body("Incomplete HTTP request")?
        }
        Err(_) => {
            // Invalid request
            Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body("Bad Request")?
        }
    };

    // Serialize the response
    let serialized = serialize_response(&response);

    // Send response
    stream.write_all(serialized.into_bytes()).await.0?;

    Ok(())
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

// A simplified response serializer
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

    // Add Content-Length if not present
    if !headers.contains_key("Content-Length") {
        result.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }

    // Finish headers and add body
    result.push_str("\r\n");
    result.push_str(&String::from_utf8_lossy(body));

    result
}
