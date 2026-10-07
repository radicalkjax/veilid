use crate::igd::gateway::parse_soap_response;
use crate::igd::search::{find_connection_service, parse_schemas, parse_search_result};
use crate::igd::xml;

const DEVICE_DESCRIPTION: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<root xmlns="urn:schemas-upnp-org:device-1-0">
  <specVersion><major>1</major><minor>0</minor></specVersion>
  <device>
    <deviceType>urn:schemas-upnp-org:device:InternetGatewayDevice:1</deviceType>
    <friendlyName>Test &amp; Gateway</friendlyName>
    <serviceList>
      <service>
        <serviceType>urn:schemas-upnp-org:service:Layer3Forwarding:1</serviceType>
        <SCPDURL>/l3f.xml</SCPDURL>
        <controlURL>/ctl/l3f</controlURL>
      </service>
    </serviceList>
    <deviceList>
      <device>
        <deviceType>urn:schemas-upnp-org:device:WANDevice:1</deviceType>
        <deviceList>
          <device>
            <deviceType>urn:schemas-upnp-org:device:WANConnectionDevice:1</deviceType>
            <serviceList>
              <service>
                <serviceType>urn:schemas-upnp-org:service:WANIPConnection:1</serviceType>
                <SCPDURL>/wanipc.xml</SCPDURL>
                <controlURL>/ctl/wanipc</controlURL>
              </service>
            </serviceList>
          </device>
        </deviceList>
      </device>
    </deviceList>
  </device>
</root>"#;

const SCPD: &str = r#"<?xml version="1.0"?>
<scpd xmlns="urn:schemas-upnp-org:service-1-0">
  <actionList>
    <action>
      <name>AddPortMapping</name>
      <argumentList>
        <argument><name>NewRemoteHost</name><direction>in</direction></argument>
        <argument><name>NewExternalPort</name><direction>in</direction></argument>
        <argument><name>NewProtocol</name><direction>in</direction></argument>
        <argument><name>NewInternalPort</name><direction>in</direction></argument>
        <argument><name>NewInternalClient</name><direction>in</direction></argument>
        <argument><name>NewEnabled</name><direction>in</direction></argument>
        <argument><name>NewPortMappingDescription</name><direction>in</direction></argument>
        <argument><name>NewLeaseDuration</name><direction>in</direction></argument>
      </argumentList>
    </action>
    <action>
      <name>GetExternalIPAddress</name>
      <argumentList>
        <argument><name>NewExternalIPAddress</name><direction>out</direction></argument>
      </argumentList>
    </action>
  </actionList>
</scpd>"#;

const SOAP_OK: &str = r#"<?xml version="1.0"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:GetExternalIPAddressResponse xmlns:u="urn:schemas-upnp-org:service:WANIPConnection:1">
      <NewExternalIPAddress>203.0.113.7</NewExternalIPAddress>
    </u:GetExternalIPAddressResponse>
  </s:Body>
</s:Envelope>"#;

const SOAP_FAULT: &str = r#"<?xml version="1.0"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <s:Fault>
      <faultcode>s:Client</faultcode>
      <faultstring>UPnPError</faultstring>
      <detail>
        <UPnPError xmlns="urn:schemas-upnp-org:control-1-0">
          <errorCode>724</errorCode>
          <errorDescription>SamePortValuesRequired</errorDescription>
        </UPnPError>
      </detail>
    </s:Fault>
  </s:Body>
</s:Envelope>"#;

pub fn test_xml_parse() {
    let root = xml::parse(DEVICE_DESCRIPTION).expect("parse device description");
    assert_eq!(root.name, "root");
    let device = root.child("device").expect("device");
    assert_eq!(device.child_text("friendlyName"), Some("Test & Gateway"));
}

pub fn test_find_connection_service() {
    let root = xml::parse(DEVICE_DESCRIPTION).expect("parse device description");
    let (scpd_url, control_url) = find_connection_service(&root).expect("connection service");
    assert_eq!(scpd_url, "/wanipc.xml");
    assert_eq!(control_url, "/ctl/wanipc");
}

pub fn test_parse_schemas() {
    let scpd = xml::parse(SCPD).expect("parse scpd");
    let schema = parse_schemas(&scpd).expect("schemas");
    assert_eq!(
        schema.get("AddPortMapping").map(Vec::len),
        Some(8),
        "in arguments only"
    );
    assert_eq!(schema.get("GetExternalIPAddress").map(Vec::len), Some(0));
}

pub fn test_parse_search_result() {
    let (addr, path) =
        parse_search_result("LOCATION: http://192.0.2.1:5000/rootDesc.xml\r\n").expect("location");
    assert_eq!(addr.to_string(), "192.0.2.1:5000");
    assert_eq!(path, "/rootDesc.xml");

    let (addr, path) =
        parse_search_result("location:http://[fe80::1]:5431/desc\r\n").expect("v6 location");
    assert_eq!(addr.port(), 5431);
    assert_eq!(path, "/desc");
    assert!(addr.is_ipv6());

    assert!(parse_search_result("content-type: text/xml\r\n").is_err());
}

pub fn test_parse_soap_response() {
    let ok = parse_soap_response(SOAP_OK, "GetExternalIPAddressResponse").expect("soap ok");
    assert_eq!(ok.child_text("NewExternalIPAddress"), Some("203.0.113.7"));

    let err = parse_soap_response(SOAP_FAULT, "AddPortMappingResponse").expect_err("soap fault");
    assert!(err.to_string().contains("724"));
    assert!(err.to_string().contains("SamePortValuesRequired"));
}

pub fn test_all() {
    test_xml_parse();
    test_find_connection_service();
    test_parse_schemas();
    test_parse_search_result();
    test_parse_soap_response();
}
