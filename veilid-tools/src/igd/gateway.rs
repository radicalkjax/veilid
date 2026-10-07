use super::*;

// SOAP fault SamePortValuesRequired
const SAME_PORT_VALUES_REQUIRED: u16 = 724;

fn random_port() -> u16 {
    crate::random::get_random_u32() as u16 % 32_767 + 32_768
}

/// A gateway found by [`super::search_gateway`].
#[derive(Clone, Debug)]
pub struct Gateway {
    /// Socket address of the gateway
    pub addr: SocketAddr,
    /// Root url of the device
    pub root_url: String,
    /// Control url of the device
    pub control_url: String,
    /// Url to get schema data from
    pub control_schema_url: String,
    /// Control schema for all actions
    pub control_schema: HashMap<String, Vec<String>>,
}

pub(crate) fn parse_soap_response(text: &str, ok: &str) -> Result<xml::Element, IgdError> {
    let envelope = xml::parse(text)?;
    let body_elem = envelope
        .child("Body")
        .ok_or_else(|| IgdError::parse("no soap body"))?;
    if let Some(ok_elem) = body_elem.child(ok) {
        return Ok(ok_elem.clone());
    }
    // decode the upnp fault, if present
    let upnp_error = body_elem
        .child("Fault")
        .and_then(|f| f.child("detail"))
        .and_then(|d| d.child("UPnPError"));
    if let Some(upnp_error) = upnp_error {
        let code = upnp_error
            .child_text("errorCode")
            .and_then(|c| c.parse::<u16>().ok())
            .ok_or_else(|| IgdError::parse("invalid upnp fault code"))?;
        let description = upnp_error
            .child_text("errorDescription")
            .unwrap_or_default()
            .to_owned();
        return Err(IgdError::soap(code, description));
    }
    Err(IgdError::parse("unrecognized soap response"))
}

impl Gateway {
    fn perform_request(
        &self,
        header: &str,
        body: &str,
        ok: &str,
    ) -> Result<xml::Element, IgdError> {
        let text = http::http_post_soap(self.addr, &self.control_url, header, body)?;
        parse_soap_response(&text, ok)
    }

    /// Get the external IP address of the gateway.
    pub fn get_external_ip(&self) -> Result<IpAddr, IgdError> {
        let response = self.perform_request(
            messages::GET_EXTERNAL_IP_HEADER,
            &messages::format_get_external_ip_message(),
            "GetExternalIPAddressResponse",
        )?;
        response
            .child_text("NewExternalIPAddress")
            .and_then(|ip| ip.parse().ok())
            .ok_or_else(|| IgdError::parse("invalid external ip address"))
    }

    /// Add a port mapping with any external port; returns the mapped external port.
    ///
    /// The `lease_duration` is in seconds; zero means infinite.
    pub fn add_any_port(
        &self,
        protocol: PortMappingProtocol,
        local_addr: SocketAddr,
        lease_duration: u32,
        description: &str,
    ) -> Result<u16, IgdError> {
        if local_addr.port() == 0 {
            return Err(IgdError::message("local port cannot be zero"));
        }

        if let Some(schema) = self.control_schema.get("AddAnyPortMapping") {
            let external_port = random_port();
            let response = self.perform_request(
                messages::ADD_ANY_PORT_MAPPING_HEADER,
                &messages::format_add_any_port_mapping_message(
                    schema,
                    protocol,
                    external_port,
                    local_addr,
                    lease_duration,
                    description,
                ),
                "AddAnyPortMappingResponse",
            )?;
            response
                .child_text("NewReservedPort")
                .and_then(|p| p.parse().ok())
                .ok_or_else(|| IgdError::parse("invalid reserved port"))
        } else {
            self.retry_add_random_port_mapping(protocol, local_addr, lease_duration, description)
        }
    }

    fn retry_add_random_port_mapping(
        &self,
        protocol: PortMappingProtocol,
        local_addr: SocketAddr,
        lease_duration: u32,
        description: &str,
    ) -> Result<u16, IgdError> {
        const ATTEMPTS: usize = 20;

        for _ in 0..ATTEMPTS {
            let external_port = random_port();
            match self.add_port_mapping(
                protocol,
                external_port,
                local_addr,
                lease_duration,
                description,
            ) {
                Ok(()) => return Ok(external_port),
                Err(e) if e.soap_code() == Some(SAME_PORT_VALUES_REQUIRED) => {
                    return self
                        .add_port_mapping(
                            protocol,
                            local_addr.port(),
                            local_addr,
                            lease_duration,
                            description,
                        )
                        .map(|()| local_addr.port());
                }
                Err(_) => {}
            }
        }
        Err(IgdError::message("no ports available"))
    }

    fn add_port_mapping(
        &self,
        protocol: PortMappingProtocol,
        external_port: u16,
        local_addr: SocketAddr,
        lease_duration: u32,
        description: &str,
    ) -> Result<(), IgdError> {
        let schema = self
            .control_schema
            .get("AddPortMapping")
            .ok_or_else(|| IgdError::message("unsupported action: AddPortMapping"))?;
        self.perform_request(
            messages::ADD_PORT_MAPPING_HEADER,
            &messages::format_add_port_mapping_message(
                schema,
                protocol,
                external_port,
                local_addr,
                lease_duration,
                description,
            ),
            "AddPortMappingResponse",
        )?;
        Ok(())
    }

    /// Add a port mapping with a specific external port.
    ///
    /// The `lease_duration` is in seconds; zero means infinite.
    pub fn add_port(
        &self,
        protocol: PortMappingProtocol,
        external_port: u16,
        local_addr: SocketAddr,
        lease_duration: u32,
        description: &str,
    ) -> Result<(), IgdError> {
        if external_port == 0 {
            return Err(IgdError::message("external port cannot be zero"));
        }
        if local_addr.port() == 0 {
            return Err(IgdError::message("local port cannot be zero"));
        }
        self.add_port_mapping(
            protocol,
            external_port,
            local_addr,
            lease_duration,
            description,
        )
    }

    /// Remove a port mapping.
    pub fn remove_port(
        &self,
        protocol: PortMappingProtocol,
        external_port: u16,
    ) -> Result<(), IgdError> {
        let schema = self
            .control_schema
            .get("DeletePortMapping")
            .ok_or_else(|| IgdError::message("unsupported action: DeletePortMapping"))?;
        self.perform_request(
            messages::DELETE_PORT_MAPPING_HEADER,
            &messages::format_delete_port_message(schema, protocol, external_port),
            "DeletePortMappingResponse",
        )?;
        Ok(())
    }
}

impl fmt::Display for Gateway {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "http://{}{}", self.addr, self.control_url)
    }
}
