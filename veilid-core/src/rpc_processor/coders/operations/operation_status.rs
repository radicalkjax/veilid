use super::*;

/// Most padding a probe carries, so a status message stays within one datagram
pub(in crate::rpc_processor) const MAX_MTU_PROBE_PADDING_LEN: usize =
    veilid_capnp::MAX_MTU_PROBE_PADDING_LEN as usize;

/// Fills a status question or answer out to one datagram size
#[derive(Debug, Clone)]
pub(in crate::rpc_processor) struct MtuProbe {
    /// Datagram payload bytes the message was sent at
    pub size: u16,
    /// Random bytes that fill the message out to `size`
    pub padding: Vec<u8>,
}

/// A probe comes straight from the node that built it, so a bad one is a protocol error
pub(in crate::rpc_processor) fn decode_mtu_probe(
    reader: &veilid_capnp::mtu_probe::Reader,
) -> Result<MtuProbe, RPCError> {
    let size = reader.get_size();
    if !(MIN_TRANSPORT_MTU..=MAX_TRANSPORT_MTU).contains(&size) {
        return Err(RPCError::protocol("mtu probe size out of range"));
    }
    let padding = if reader.has_padding() {
        let padding_reader = reader.get_padding()?;
        rpc_ignore_max_len!(padding_reader, MAX_MTU_PROBE_PADDING_LEN);
        padding_reader.to_vec()
    } else {
        Vec::new()
    };
    // The padding, its frame header and the envelope around it fill the datagram, never more
    if padding.len() + FRAME_HEADER_LEN + ENV0_MIN_ENVELOPE_SIZE > usize::from(size) {
        return Err(RPCError::protocol("mtu probe padding exceeds its size"));
    }
    Ok(MtuProbe { size, padding })
}

fn encode_mtu_probe(mtu_probe: &MtuProbe, builder: &mut veilid_capnp::mtu_probe::Builder) {
    builder.set_size(mtu_probe.size);
    if !mtu_probe.padding.is_empty() {
        builder.set_padding(&mtu_probe.padding);
    }
}

#[derive(Debug, Clone)]
pub(in crate::rpc_processor) struct RPCOperationStatusQ {
    node_status: Option<NodeStatus>,
    mtu_probe: Option<MtuProbe>,
}

impl RPCOperationStatusQ {
    pub fn new(node_status: Option<NodeStatus>, mtu_probe: Option<MtuProbe>) -> Self {
        Self {
            node_status,
            mtu_probe,
        }
    }

    // pub async fn validate(
    //     &self,
    //     _validate_context: &RPCValidateContext<'_>,
    // ) -> Result<(), RPCError> {
    //     Ok(())
    // }

    // pub fn node_status(&self) -> Option<&NodeStatus> {
    //     self.node_status.as_ref()
    // }
    pub fn mtu_probe_slot(&mut self) -> &mut Option<MtuProbe> {
        &mut self.mtu_probe
    }

    pub fn destructure(self) -> (Option<NodeStatus>, Option<MtuProbe>) {
        (self.node_status, self.mtu_probe)
    }

    pub fn decode(
        _decode_context: &RPCDecodeContext,
        reader: &veilid_capnp::operation_status_q::Reader,
    ) -> Result<Self, RPCError> {
        let node_status = if reader.has_node_status() {
            let ns_reader = reader.get_node_status()?;
            let node_status = decode_node_status(&ns_reader)?;
            Some(node_status)
        } else {
            None
        };
        let mtu_probe = if reader.has_mtu_probe() {
            Some(decode_mtu_probe(&reader.get_mtu_probe()?)?)
        } else {
            None
        };
        Ok(Self::new(node_status, mtu_probe))
    }
    pub fn encode(
        &self,
        builder: &mut veilid_capnp::operation_status_q::Builder,
    ) -> Result<(), RPCError> {
        if let Some(ns) = &self.node_status {
            let mut ns_builder = builder.reborrow().init_node_status();
            encode_node_status(ns, &mut ns_builder)?;
        }
        if let Some(mp) = &self.mtu_probe {
            let mut mp_builder = builder.reborrow().init_mtu_probe();
            encode_mtu_probe(mp, &mut mp_builder);
        }
        Ok(())
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////

#[derive(Debug, Clone)]
pub(in crate::rpc_processor) struct RPCOperationStatusA {
    node_status: Option<NodeStatus>,
    sender_info: Option<SenderInfo>,
    mtu_probe: Option<MtuProbe>,
}

impl RPCOperationStatusA {
    pub fn new(
        node_status: Option<NodeStatus>,
        sender_info: Option<SenderInfo>,
        mtu_probe: Option<MtuProbe>,
    ) -> Self {
        Self {
            node_status,
            sender_info,
            mtu_probe,
        }
    }

    // pub async fn validate(
    //     &self,
    //     _validate_context: &RPCValidateContext<'_>,
    // ) -> Result<(), RPCError> {
    //     Ok(())
    // }

    pub fn mtu_probe_slot(&mut self) -> &mut Option<MtuProbe> {
        &mut self.mtu_probe
    }

    pub fn destructure(self) -> (Option<NodeStatus>, Option<SenderInfo>, Option<MtuProbe>) {
        (self.node_status, self.sender_info, self.mtu_probe)
    }

    pub fn decode(
        _decode_context: &RPCDecodeContext,
        reader: &veilid_capnp::operation_status_a::Reader,
    ) -> Result<Self, RPCError> {
        let node_status = if reader.has_node_status() {
            let ns_reader = reader.get_node_status()?;
            let node_status = decode_node_status(&ns_reader)?;
            Some(node_status)
        } else {
            None
        };

        let sender_info = if reader.has_sender_info() {
            let si_reader = reader.get_sender_info()?;
            let sender_info = decode_sender_info(&si_reader)?;
            Some(sender_info)
        } else {
            None
        };

        let mtu_probe = if reader.has_mtu_probe() {
            Some(decode_mtu_probe(&reader.get_mtu_probe()?)?)
        } else {
            None
        };

        Ok(Self::new(node_status, sender_info, mtu_probe))
    }
    pub fn encode(
        &self,
        builder: &mut veilid_capnp::operation_status_a::Builder,
    ) -> Result<(), RPCError> {
        if let Some(ns) = &self.node_status {
            let mut ns_builder = builder.reborrow().init_node_status();
            encode_node_status(ns, &mut ns_builder)?;
        }
        if let Some(si) = &self.sender_info {
            let mut si_builder = builder.reborrow().init_sender_info();
            encode_sender_info(si, &mut si_builder)?;
        }
        if let Some(mp) = &self.mtu_probe {
            let mut mp_builder = builder.reborrow().init_mtu_probe();
            encode_mtu_probe(mp, &mut mp_builder);
        }
        Ok(())
    }
}
