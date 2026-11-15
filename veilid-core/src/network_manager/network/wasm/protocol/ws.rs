use super::*;
use futures_util::{SinkExt, StreamExt};
use send_wrapper::*;
use std::io;
use ws_stream_wasm::*;

struct WebsocketNetworkConnectionInner {
    ws_meta: WsMeta,
    ws_stream: CloneStream<WsStream>,
}

#[derive(Clone)]
pub struct WebsocketNetworkConnection {
    registry: VeilidComponentRegistry,
    flow: Flow,
    inner: Arc<WebsocketNetworkConnectionInner>,
}

impl fmt::Debug for WebsocketNetworkConnection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebsocketNetworkConnection")
            .field("flow", &self.flow)
            .finish()
    }
}

impl_veilid_component_accessors!(WebsocketNetworkConnection);

impl WebsocketNetworkConnection {
    pub fn new(
        registry: VeilidComponentRegistry,
        flow: Flow,
        ws_meta: WsMeta,
        ws_stream: WsStream,
    ) -> Self {
        Self {
            registry,
            flow,
            inner: Arc::new(WebsocketNetworkConnectionInner {
                ws_meta,
                ws_stream: CloneStream::new(ws_stream),
            }),
        }
    }

    pub fn flow(&self) -> Flow {
        self.flow
    }

    #[cfg_attr(
        feature = "verbose-tracing",
        instrument(level = "trace", err, skip(self), fields(__VEILID_LOG_KEY = self.log_key()))
    )]
    pub async fn close(&self) -> io::Result<NetworkResult<()>> {
        let timeout_ms = self
            .config()
            .internal()
            .network
            .connection_initial_timeout_ms;

        // A close that fails must not report success
        match timeout(timeout_ms, self.inner.ws_meta.close()).await {
            Ok(Ok(_)) => Ok(NetworkResult::value(())),
            Ok(Err(e)) => Err::<(), _>(ws_err_to_io_error(e)).into_network_result(),
            Err(_) => Ok(NetworkResult::timeout()),
        }
    }

    #[cfg_attr(feature = "instrument", instrument(level = "trace", target="protocol", err, skip(self, message), fields(network_result, message.len = message.len())))]
    pub async fn send(&self, message: Bytes) -> io::Result<NetworkResult<()>> {
        if message.len() > MAX_MESSAGE_SIZE {
            bail_io_error_other!("sending too large WS message");
        }
        let out = SendWrapper::new(
            self.inner
                .ws_stream
                .clone()
                .send(WsMessage::Binary(message.to_vec())),
        )
        .await
        .map_err(ws_err_to_io_error)
        .into_network_result()?;

        #[cfg(feature = "verbose-tracing")]
        tracing::Span::current().record("network_result", &tracing::field::display(&out));
        Ok(out)
    }

    #[cfg_attr(feature = "instrument", instrument(level = "trace", target="protocol", err, skip(self), fields(network_result, ret.len)))]
    pub async fn recv(&self) -> io::Result<NetworkResult<Bytes>> {
        let out = match SendWrapper::new(self.inner.ws_stream.clone().next()).await {
            Some(WsMessage::Binary(v)) => {
                if v.len() > MAX_MESSAGE_SIZE {
                    return Ok(NetworkResult::invalid_message("too large ws message"));
                }
                NetworkResult::Value(Bytes::from(v))
            }
            Some(_) => NetworkResult::no_connection_other(io::Error::new(
                io::ErrorKind::ConnectionReset,
                "Unexpected WS message type",
            )),
            None => {
                return Ok(NetworkResult::no_connection(io::Error::new(
                    io::ErrorKind::ConnectionReset,
                    "WS stream closed",
                )));
            }
        };
        #[cfg(feature = "verbose-tracing")]
        tracing::Span::current().record("network_result", &tracing::field::display(&out));
        Ok(out)
    }
}

///////////////////////////////////////////////////////////

pub(in crate::network_manager) struct WebsocketProtocolHandler {}

impl WebsocketProtocolHandler {
    #[cfg_attr(
        feature = "instrument",
        instrument(level = "trace", target = "protocol", ret, err)
    )]
    pub async fn connect(
        registry: VeilidComponentRegistry,
        dial_info: DialInfo,
        timeout_ms: u32,
    ) -> io::Result<NetworkResult<ProtocolNetworkConnection>> {
        // Split dial info up
        let (_tls, scheme) = match dial_info {
            DialInfo::WS(_) => (false, "ws"),
            #[cfg(feature = "enable-protocol-wss")]
            DialInfo::WSS(_) => (true, "wss"),
            _ => bail_io_error_other!("invalid dialinfo for WS/WSS protocol"),
        };
        let request = dial_info.request().unwrap_or_log();
        let split_url = SplitUrl::from_str(&request).map_err(to_io_error_other)?;
        if split_url.scheme != scheme {
            bail_io_error_other!("invalid websocket url scheme");
        }

        let socket_address = dial_info.socket_address();
        let connect_request = if socket_address.ip_addr().is_unspecified() {
            request
        } else {
            let ip_based_url = SplitUrl::new(
                scheme,
                split_url.userinfo.clone(),
                SplitUrlHost::IpAddr(socket_address.ip_addr()),
                Some(socket_address.port()),
                split_url.path.clone(),
            );
            ip_based_url.to_string()
        };

        let fut = SendWrapper::new(timeout(timeout_ms, async move {
            WsMeta::connect(connect_request, None)
                .await
                .map_err(ws_err_to_io_error)
        }));

        let (wsmeta, wsio) = network_result_try!(network_result_try!(fut
            .await
            .into_network_result())
        .into_network_result()?);

        // Make our flow
        let wnc = WebsocketNetworkConnection::new(
            registry.clone(),
            Flow::new_no_local(dial_info.peer_address()),
            wsmeta,
            wsio,
        );
        Ok(NetworkResult::Value(
            registry
                .network_manager()
                .chaos()
                // A browser exposes no segment size
                .wrap_connection(Box::new(WasmProtocolNetworkConnection::Ws(wnc)), None),
        ))
    }
}
