use super::*;

const HTTP_TIMEOUT_SECS: u64 = 10;

fn url_for(addr: &SocketAddr, path: &str) -> String {
    match addr {
        SocketAddr::V4(v4) => format!("http://{}:{}{}", v4.ip(), v4.port(), path),
        SocketAddr::V6(v6) => format!("http://[{}]:{}{}", v6.ip(), v6.port(), path),
    }
}

pub(super) fn http_get(addr: SocketAddr, path: &str) -> Result<Vec<u8>, IgdError> {
    let response = minreq::get(url_for(&addr, path))
        .with_timeout(HTTP_TIMEOUT_SECS)
        .send()
        .map_err(|e| IgdError::http(e.to_string()))?;
    if !(200..300).contains(&response.status_code) {
        return Err(IgdError::http(format!(
            "http status {}",
            response.status_code
        )));
    }
    Ok(response.into_bytes())
}

// SOAP faults arrive with non-2xx statuses; the body is returned regardless so they can be decoded
pub(super) fn http_post_soap(
    addr: SocketAddr,
    path: &str,
    soap_action: &str,
    body: &str,
) -> Result<String, IgdError> {
    let response = minreq::post(url_for(&addr, path))
        .with_header("SOAPAction", soap_action)
        .with_header("Content-Type", "text/xml")
        .with_timeout(HTTP_TIMEOUT_SECS)
        .with_body(body)
        .send()
        .map_err(|e| IgdError::http(e.to_string()))?;
    response
        .as_str()
        .map(str::to_owned)
        .map_err(|e| IgdError::http(e.to_string()))
}
