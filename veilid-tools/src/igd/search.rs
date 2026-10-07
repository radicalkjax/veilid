use super::*;

const CONNECTION_SERVICES: [&str; 3] = [
    "urn:schemas-upnp-org:service:WANPPPConnection:1",
    "urn:schemas-upnp-org:service:WANIPConnection:1",
    "urn:schemas-upnp-org:service:WANIPConnection:2",
];

/// Search for a gateway on the network described by `options`.
pub fn search_gateway(options: SearchOptions) -> Result<Gateway, IgdError> {
    let socket =
        UdpSocket::bind(options.bind_addr).map_err(|e| IgdError::message(e.to_string()))?;
    socket
        .set_read_timeout(options.timeout)
        .map_err(|e| IgdError::message(e.to_string()))?;

    let request = messages::SEARCH_REQUEST
        .replace("$BROADCAST_ADDRESS", &options.broadcast_address.to_string());
    socket
        .send_to(request.as_bytes(), options.broadcast_address)
        .map_err(|e| IgdError::message(format!("ssdp send failed: {}", e)))?;

    loop {
        let mut buf = [0u8; 1500];
        let (read, _) = socket
            .recv_from(&mut buf)
            .map_err(|e| IgdError::message(format!("no gateway found: {}", e)))?;
        let Ok(text) = core::str::from_utf8(&buf[..read]) else {
            continue;
        };
        let (addr, root_path) = match parse_search_result(text) {
            Ok(v) => v,
            Err(e) => {
                log::debug!("ssdp response skipped: {}", e);
                continue;
            }
        };
        match get_gateway(addr, &root_path) {
            Ok(gateway) => return Ok(gateway),
            Err(e) => {
                log::debug!("gateway description skipped: {}", e);
                continue;
            }
        }
    }
}

// LOCATION header value -> (gateway address, description path)
pub(crate) fn parse_search_result(text: &str) -> Result<(SocketAddr, String), IgdError> {
    for line in text.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("location") {
            return parse_http_url(value.trim());
        }
    }
    Err(IgdError::parse("no location header"))
}

fn parse_http_url(url: &str) -> Result<(SocketAddr, String), IgdError> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| IgdError::parse("not an http url"))?;
    let (host_port, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = if let Some(v6) = host_port.strip_prefix('[') {
        let (host, after) = v6
            .split_once(']')
            .ok_or_else(|| IgdError::parse("unterminated ipv6 literal"))?;
        let port = match after.strip_prefix(':') {
            Some(p) => p.parse().map_err(|_| IgdError::parse("invalid port"))?,
            None => 80,
        };
        // scope ids are not routable information for the description fetch
        (host.split('%').next().unwrap_or(host), port)
    } else {
        match host_port.split_once(':') {
            Some((host, p)) => (
                host,
                p.parse().map_err(|_| IgdError::parse("invalid port"))?,
            ),
            None => (host_port, 80),
        }
    };
    let ip: IpAddr = host
        .parse()
        .map_err(|_| IgdError::parse("host is not an ip literal"))?;
    Ok((SocketAddr::new(ip, port), path.to_owned()))
}

fn get_gateway(addr: SocketAddr, root_path: &str) -> Result<Gateway, IgdError> {
    let description = http::http_get(addr, root_path)?;
    let description =
        core::str::from_utf8(&description).map_err(|_| IgdError::parse("description not utf-8"))?;
    let root = xml::parse(description)?;
    let (control_schema_url, control_url) = find_connection_service(&root)
        .ok_or_else(|| IgdError::parse("no connection service in device description"))?;

    let scpd = http::http_get(addr, &control_schema_url)?;
    let scpd = core::str::from_utf8(&scpd).map_err(|_| IgdError::parse("scpd not utf-8"))?;
    let control_schema = parse_schemas(&xml::parse(scpd)?)?;

    Ok(Gateway {
        addr,
        root_url: root_path.to_owned(),
        control_url,
        control_schema_url,
        control_schema,
    })
}

// Walk nested device/deviceList/serviceList structures for a WAN connection service
pub(crate) fn find_connection_service(element: &xml::Element) -> Option<(String, String)> {
    if let Some(service_list) = element.child("serviceList") {
        for service in service_list.children_named("service") {
            let Some(service_type) = service.child_text("serviceType") else {
                continue;
            };
            if CONNECTION_SERVICES.contains(&service_type) {
                let scpd_url = service.child_text("SCPDURL")?.to_owned();
                let control_url = service.child_text("controlURL")?.to_owned();
                return Some((scpd_url, control_url));
            }
        }
    }
    if let Some(device_list) = element.child("deviceList") {
        for device in device_list.children_named("device") {
            if let Some(found) = find_connection_service(device) {
                return Some(found);
            }
        }
    }
    if let Some(device) = element.child("device") {
        return find_connection_service(device);
    }
    None
}

// SCPD actionList -> action name to input argument names
pub(crate) fn parse_schemas(scpd: &xml::Element) -> Result<HashMap<String, Vec<String>>, IgdError> {
    let action_list = scpd
        .child("actionList")
        .ok_or_else(|| IgdError::parse("no actionList in scpd"))?;
    let mut schema = HashMap::new();
    for action in action_list.children_named("action") {
        let Some(name) = action.child_text("name") else {
            continue;
        };
        let mut args = Vec::new();
        if let Some(argument_list) = action.child("argumentList") {
            for argument in argument_list.children_named("argument") {
                if argument.child_text("direction") == Some("in") {
                    if let Some(arg_name) = argument.child_text("name") {
                        args.push(arg_name.to_owned());
                    }
                }
            }
        }
        schema.insert(name.to_owned(), args);
    }
    Ok(schema)
}
