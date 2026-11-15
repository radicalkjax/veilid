use super::*;
use futures_util::{AsyncReadExt, AsyncWriteExt};

cfg_if! {
    if #[cfg(feature="rt-async-std")] {
        fn futures_io_stream(stream: TcpStream) -> TcpStream {
            stream
        }
    } else if #[cfg(feature="rt-tokio")] {
        // tokio's stream reaches the futures-io traits through compat
        fn futures_io_stream(stream: TcpStream) -> tokio_util::compat::Compat<TcpStream> {
            use tokio_util::compat::TokioAsyncReadCompatExt;
            stream.compat()
        }
    } else {
        compile_error!("needs executor implementation");
    }
}

/// Bound on one request, from connect to the last byte of the response
const HTTP_TIMEOUT_MS: u32 = 10_000;
/// Headers a response may carry before it fails to parse
const MAX_HEADERS: usize = 64;

pub(super) async fn http_get(addr: SocketAddr, path: &str) -> Result<Vec<u8>, IgdError> {
    let request = format!("GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    let (status, body) = http_request(addr, request.into_bytes()).await?;
    if !(200..300).contains(&status) {
        return Err(IgdError::http(format!("http status {}", status)));
    }
    Ok(body)
}

// SOAP faults arrive with non-2xx statuses; the body is returned regardless so they can be decoded
pub(super) async fn http_post_soap(
    addr: SocketAddr,
    path: &str,
    soap_action: &str,
    body: &str,
) -> Result<String, IgdError> {
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {addr}\r\nSOAPAction: {soap_action}\r\nContent-Type: text/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let (_, body) = http_request(addr, request.into_bytes()).await?;
    String::from_utf8(body).map_err(|e| IgdError::http(e.to_string()))
}

async fn http_request(addr: SocketAddr, request: Vec<u8>) -> Result<(u16, Vec<u8>), IgdError> {
    timeout(HTTP_TIMEOUT_MS, exchange(addr, request))
        .await
        .map_err(|_| IgdError::http("request timed out"))?
}

async fn exchange(addr: SocketAddr, request: Vec<u8>) -> Result<(u16, Vec<u8>), IgdError> {
    let stream = connect_async_tcp_stream(None, addr, HTTP_TIMEOUT_MS, None)
        .await
        .map_err(|e| IgdError::http(e.to_string()))?
        .into_timeout_error()
        .map_err(|_| IgdError::http("connect timed out"))?;
    let mut stream = futures_io_stream(stream);
    stream
        .write_all(&request)
        .await
        .map_err(|e| IgdError::http(e.to_string()))?;

    let mut response = Vec::new();
    // On the heap, so the request future stays small
    let mut buf = vec![0u8; 4096];
    loop {
        let read = stream
            .read(&mut buf)
            .await
            .map_err(|e| IgdError::http(e.to_string()))?;
        response.extend_from_slice(&buf[..read]);
        // Framed by length or chunks, so a router that keeps the connection open is not waited on
        if let Some(parsed) = parse_response(&response, read == 0)? {
            return Ok(parsed);
        }
        if read == 0 {
            return Err(IgdError::http("connection closed before a response"));
        }
    }
}

/// The status and body once `response` holds a whole response, or `None` while more is due.
/// `closed` means the gateway hung up, which ends a body that has no length.
pub(crate) fn parse_response(
    response: &[u8],
    closed: bool,
) -> Result<Option<(u16, Vec<u8>)>, IgdError> {
    let mut headers = [httparse::EMPTY_HEADER; MAX_HEADERS];
    let mut parsed = httparse::Response::new(&mut headers);
    let head_len = match parsed
        .parse(response)
        .map_err(|e| IgdError::http(e.to_string()))?
    {
        httparse::Status::Complete(head_len) => head_len,
        httparse::Status::Partial if closed => {
            return Err(IgdError::http("response ended in its headers"))
        }
        httparse::Status::Partial => return Ok(None),
    };
    let status = parsed
        .code
        .ok_or_else(|| IgdError::http("no status code"))?;
    let header = |name: &str| {
        parsed
            .headers
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case(name))
            .map(|h| String::from_utf8_lossy(h.value).trim().to_ascii_lowercase())
    };
    let body = &response[head_len..];

    if header("transfer-encoding").is_some_and(|te| te.contains("chunked")) {
        return match decode_chunked(body)? {
            Some(body) => Ok(Some((status, body))),
            None if closed => Err(IgdError::http("response ended in a chunk")),
            None => Ok(None),
        };
    }
    if let Some(len) = header("content-length") {
        let len: usize = len
            .parse()
            .map_err(|_| IgdError::http("invalid content length"))?;
        if body.len() >= len {
            return Ok(Some((status, body[..len].to_vec())));
        }
        if closed {
            return Err(IgdError::http("response shorter than its content length"));
        }
        return Ok(None);
    }
    Ok(closed.then(|| (status, body.to_vec())))
}

/// The body of a chunked response once its last chunk has arrived, or `None` before
fn decode_chunked(mut body: &[u8]) -> Result<Option<Vec<u8>>, IgdError> {
    let mut out = Vec::new();
    loop {
        let (size_len, size) = match httparse::parse_chunk_size(body)
            .map_err(|_| IgdError::http("invalid chunk size"))?
        {
            httparse::Status::Complete(v) => v,
            httparse::Status::Partial => return Ok(None),
        };
        body = &body[size_len..];
        if size == 0 {
            return Ok(Some(out));
        }
        let size = usize::try_from(size).map_err(|_| IgdError::http("chunk too large"))?;
        // The chunk's data, then its CRLF
        if body.len() < size + 2 {
            return Ok(None);
        }
        out.extend_from_slice(&body[..size]);
        body = &body[size + 2..];
    }
}
