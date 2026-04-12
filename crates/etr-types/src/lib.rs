#![no_std]

use core::mem;

pub const TC_FORWARD_RULES_MAP: &str = "ETR_TC_FORWARD_RULES";
pub const TC_FLOW_STATE_MAP: &str = "ETR_TC_FLOW_STATE";
pub const TC_INGRESS_PROGRAM_NAME: &str = "etr_ingress";
pub const TC_EGRESS_PROGRAM_NAME: &str = "etr_egress";
pub const MAX_FORWARD_RULES: u32 = 4096;
pub const MAX_FLOW_STATES: u32 = 65535;

pub const IPV4_PROTOCOL_VERSION: u8 = 4;
pub const ETH_P_IP: u16 = 0x0800;
pub const IPPROTO_TCP: u8 = 6;
pub const IPPROTO_UDP: u8 = 17;

pub const BPF_F_RECOMPUTE_CSUM: u64 = 1 << 0;
pub const BPF_F_INVALIDATE_HASH: u64 = 1 << 1;
pub const BPF_F_HDR_FIELD_MASK: u64 = 0x0f;
pub const BPF_F_PSEUDO_HDR: u64 = 1 << 4;
pub const BPF_F_MARK_MANGLED_0: u64 = 1 << 5;

pub const CSUM_REWRITE_FLAGS: u64 = BPF_F_RECOMPUTE_CSUM | BPF_F_INVALIDATE_HASH;
pub const L4_PORT_CSUM_FLAGS: u64 = 2 | BPF_F_MARK_MANGLED_0;
pub const L4_IPV4_ADDR_CSUM_FLAGS: u64 = 4 | BPF_F_PSEUDO_HDR | BPF_F_MARK_MANGLED_0;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportProtocol {
    Tcp = IPPROTO_TCP,
    Udp = IPPROTO_UDP,
}

impl TransportProtocol {
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnatMode {
    Masquerade = 1,
}

impl SnatMode {
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ForwardRuleKey {
    pub protocol: u8,
    pub reserved: [u8; 3],
    pub listen_addr_be: u32,
    pub listen_port_be: u16,
    pub padding: u16,
}

impl ForwardRuleKey {
    pub const fn new(
        protocol: TransportProtocol,
        listen_addr_be: u32,
        listen_port_be: u16,
    ) -> Self {
        Self {
            protocol: protocol.as_u8(),
            reserved: [0; 3],
            listen_addr_be,
            listen_port_be,
            padding: 0,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForwardRuleValue {
    pub backend_addr_be: u32,
    pub backend_port_be: u16,
    pub snat_mode: u8,
    pub reserved: u8,
}

impl ForwardRuleValue {
    pub const fn new(backend_addr_be: u32, backend_port_be: u16, snat_mode: SnatMode) -> Self {
        Self {
            backend_addr_be,
            backend_port_be,
            snat_mode: snat_mode.as_u8(),
            reserved: 0,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FlowStateKey {
    pub protocol: u8,
    pub reserved: [u8; 3],
    pub src_addr_be: u32,
    pub dst_addr_be: u32,
    pub src_port_be: u16,
    pub dst_port_be: u16,
}

impl FlowStateKey {
    pub const fn new(
        protocol: TransportProtocol,
        src_addr_be: u32,
        dst_addr_be: u32,
        src_port_be: u16,
        dst_port_be: u16,
    ) -> Self {
        Self {
            protocol: protocol.as_u8(),
            reserved: [0; 3],
            src_addr_be,
            dst_addr_be,
            src_port_be,
            dst_port_be,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlowStateValue {
    pub rewrite_src_addr_be: u32,
    pub rewrite_src_port_be: u16,
    pub rewrite_dst_addr_be: u32,
    pub rewrite_dst_port_be: u16,
    pub snat_mode: u8,
    pub reserved: [u8; 3],
}

impl FlowStateValue {
    pub const fn new(
        rewrite_src_addr_be: u32,
        rewrite_src_port_be: u16,
        rewrite_dst_addr_be: u32,
        rewrite_dst_port_be: u16,
        snat_mode: SnatMode,
    ) -> Self {
        Self {
            rewrite_src_addr_be,
            rewrite_src_port_be,
            rewrite_dst_addr_be,
            rewrite_dst_port_be,
            snat_mode: snat_mode.as_u8(),
            reserved: [0; 3],
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct EthHdr {
    pub dst: [u8; 6],
    pub src: [u8; 6],
    pub ether_type_be: u16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Ipv4Hdr {
    pub version_ihl: u8,
    pub tos: u8,
    pub total_len_be: u16,
    pub id_be: u16,
    pub frag_off_be: u16,
    pub ttl: u8,
    pub protocol: u8,
    pub check_be: u16,
    pub saddr_be: u32,
    pub daddr_be: u32,
}

impl Ipv4Hdr {
    pub const fn header_len_bytes(self) -> usize {
        ((self.version_ihl & 0x0f) as usize) * 4
    }

    pub const fn version(self) -> u8 {
        self.version_ihl >> 4
    }

    pub const fn is_fragmented(self) -> bool {
        (u16::from_be(self.frag_off_be) & 0x3fff) != 0
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TcpHdr {
    pub source_be: u16,
    pub dest_be: u16,
    pub seq_be: u32,
    pub ack_seq_be: u32,
    pub offset_flags_be: u16,
    pub window_be: u16,
    pub check_be: u16,
    pub urg_ptr_be: u16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct UdpHdr {
    pub source_be: u16,
    pub dest_be: u16,
    pub len_be: u16,
    pub check_be: u16,
}

pub const ETH_HDR_LEN: usize = mem::size_of::<EthHdr>();
pub const IPV4_HDR_LEN: usize = mem::size_of::<Ipv4Hdr>();
pub const TCP_HDR_LEN: usize = mem::size_of::<TcpHdr>();
pub const UDP_HDR_LEN: usize = mem::size_of::<UdpHdr>();
