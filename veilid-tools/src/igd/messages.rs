use super::*;

pub const SEARCH_REQUEST: &str = "M-SEARCH * HTTP/1.1\r
Host:$BROADCAST_ADDRESS\r
ST:urn:schemas-upnp-org:device:InternetGatewayDevice:1\r
Man:\"ssdp:discover\"\r
MX:3\r\n\r\n";

pub const GET_EXTERNAL_IP_HEADER: &str =
    r#""urn:schemas-upnp-org:service:WANIPConnection:1#GetExternalIPAddress""#;

pub const ADD_ANY_PORT_MAPPING_HEADER: &str =
    r#""urn:schemas-upnp-org:service:WANIPConnection:1#AddAnyPortMapping""#;

pub const ADD_PORT_MAPPING_HEADER: &str =
    r#""urn:schemas-upnp-org:service:WANIPConnection:1#AddPortMapping""#;

pub const DELETE_PORT_MAPPING_HEADER: &str =
    r#""urn:schemas-upnp-org:service:WANIPConnection:1#DeletePortMapping""#;

const MESSAGE_HEAD: &str = r#"<?xml version="1.0"?>
<s:Envelope s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/" xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
<s:Body>"#;

const MESSAGE_TAIL: &str = r#"</s:Body>
</s:Envelope>"#;

fn format_message(body: String) -> String {
    format!("{MESSAGE_HEAD}{body}{MESSAGE_TAIL}")
}

// Emits only the arguments the gateway's SCPD declares for the action, in declaration order
fn format_args(schema: &[String], value_for: impl Fn(&str) -> Option<String>) -> String {
    schema
        .iter()
        .filter_map(|argument| {
            let Some(value) = value_for(argument) else {
                log::warn!("Unknown argument: {}", argument);
                return None;
            };
            Some(format!("<{argument}>{value}</{argument}>"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn mapping_arg_value(
    argument: &str,
    protocol: PortMappingProtocol,
    external_port: u16,
    local_addr: SocketAddr,
    lease_duration: u32,
    description: &str,
) -> Option<String> {
    Some(match argument {
        "NewEnabled" => 1.to_string(),
        "NewExternalPort" => external_port.to_string(),
        "NewInternalClient" => local_addr.ip().to_string(),
        "NewInternalPort" => local_addr.port().to_string(),
        "NewLeaseDuration" => lease_duration.to_string(),
        "NewPortMappingDescription" => description.to_string(),
        "NewProtocol" => protocol.to_string(),
        "NewRemoteHost" => String::new(),
        _ => return None,
    })
}

pub fn format_get_external_ip_message() -> String {
    format_message(
        r#"<m:GetExternalIPAddress xmlns:m="urn:schemas-upnp-org:service:WANIPConnection:1">
        </m:GetExternalIPAddress>"#
            .to_owned(),
    )
}

pub fn format_add_any_port_mapping_message(
    schema: &[String],
    protocol: PortMappingProtocol,
    external_port: u16,
    local_addr: SocketAddr,
    lease_duration: u32,
    description: &str,
) -> String {
    let args = format_args(schema, |argument| {
        mapping_arg_value(
            argument,
            protocol,
            external_port,
            local_addr,
            lease_duration,
            description,
        )
    });
    format_message(format!(
        r#"<u:AddAnyPortMapping xmlns:u="urn:schemas-upnp-org:service:WANIPConnection:1">
        {args}
        </u:AddAnyPortMapping>"#,
    ))
}

pub fn format_add_port_mapping_message(
    schema: &[String],
    protocol: PortMappingProtocol,
    external_port: u16,
    local_addr: SocketAddr,
    lease_duration: u32,
    description: &str,
) -> String {
    let args = format_args(schema, |argument| {
        mapping_arg_value(
            argument,
            protocol,
            external_port,
            local_addr,
            lease_duration,
            description,
        )
    });
    format_message(format!(
        r#"<u:AddPortMapping xmlns:u="urn:schemas-upnp-org:service:WANIPConnection:1">
        {args}
        </u:AddPortMapping>"#
    ))
}

pub fn format_delete_port_message(
    schema: &[String],
    protocol: PortMappingProtocol,
    external_port: u16,
) -> String {
    let args = format_args(schema, |argument| {
        Some(match argument {
            "NewExternalPort" => external_port.to_string(),
            "NewProtocol" => protocol.to_string(),
            "NewRemoteHost" => String::new(),
            _ => return None,
        })
    });
    format_message(format!(
        r#"<u:DeletePortMapping xmlns:u="urn:schemas-upnp-org:service:WANIPConnection:1">
        {args}
        </u:DeletePortMapping>"#
    ))
}
