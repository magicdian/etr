#![no_std]
#![no_main]

use core::mem::offset_of;

use aya_ebpf::bindings::{TC_ACT_PIPE, TC_ACT_SHOT};
use aya_ebpf::macros::{classifier, map};
use aya_ebpf::maps::{HashMap, LruHashMap};
use aya_ebpf::programs::TcContext;
use etr_types::{
    BPF_F_MARK_MANGLED_0, ETH_HDR_LEN, ETH_P_IP, EthHdr, FlowStateKey, FlowStateValue,
    ForwardRuleKey, ForwardRuleValue, IPPROTO_TCP, IPPROTO_UDP, IPV4_PROTOCOL_VERSION, Ipv4Hdr,
    L4_IPV4_ADDR_CSUM_FLAGS, L4_PORT_CSUM_FLAGS, MAX_FLOW_STATES, MAX_FORWARD_RULES, TcpHdr,
    TransportProtocol, UdpHdr,
};

#[map]
static ETR_TC_FORWARD_RULES: HashMap<ForwardRuleKey, ForwardRuleValue> =
    HashMap::with_max_entries(MAX_FORWARD_RULES, 0);

#[map]
static ETR_TC_FLOW_STATE: LruHashMap<FlowStateKey, FlowStateValue> =
    LruHashMap::with_max_entries(MAX_FLOW_STATES, 0);

#[classifier]
pub fn etr_ingress(ctx: TcContext) -> i32 {
    match try_etr_ingress(ctx) {
        Ok(ret) => ret,
        Err(ret) => ret,
    }
}

#[classifier]
pub fn etr_egress(ctx: TcContext) -> i32 {
    match try_etr_egress(ctx) {
        Ok(ret) => ret,
        Err(ret) => ret,
    }
}

fn try_etr_ingress(mut ctx: TcContext) -> Result<i32, i32> {
    let eth = load_eth(&ctx)?;
    if u16::from_be(eth.ether_type_be) != ETH_P_IP {
        return Ok(TC_ACT_PIPE);
    }

    let ip = load_ipv4(&ctx)?;
    if ip.version() != IPV4_PROTOCOL_VERSION || ip.is_fragmented() {
        return Ok(TC_ACT_PIPE);
    }

    let protocol = transport_protocol(ip.protocol)?;
    let l4_offset = ETH_HDR_LEN + ip.header_len_bytes();
    let (src_port_be, dst_port_be, checksum_offset) = load_ports(&ctx, protocol, l4_offset)?;

    let exact_key = ForwardRuleKey::new(protocol, ip.daddr_be, dst_port_be);
    let wildcard_key = ForwardRuleKey::new(protocol, 0, dst_port_be);
    let rule = lookup_rule(&exact_key).or_else(|| lookup_rule(&wildcard_key));
    let rule = match rule {
        Some(rule) => rule,
        None => return Ok(TC_ACT_PIPE),
    };

    let flow_key = FlowStateKey::new(
        protocol,
        rule.backend_addr_be,
        ip.saddr_be,
        rule.backend_port_be,
        src_port_be,
    );
    let flow_value = FlowStateValue::new(ip.daddr_be, dst_port_be, etr_types::SnatMode::Masquerade);

    let _ = ETR_TC_FLOW_STATE.insert(&flow_key, &flow_value, 0);

    rewrite_ipv4_addr(
        &mut ctx,
        ETH_HDR_LEN + offset_of!(Ipv4Hdr, daddr_be),
        ETH_HDR_LEN + offset_of!(Ipv4Hdr, check_be),
        checksum_offset,
        ip.daddr_be,
        rule.backend_addr_be,
    )?;
    rewrite_l4_port(
        &mut ctx,
        port_offset(protocol, l4_offset, false),
        checksum_offset,
        dst_port_be,
        rule.backend_port_be,
    )?;

    Ok(TC_ACT_PIPE)
}

fn try_etr_egress(mut ctx: TcContext) -> Result<i32, i32> {
    let eth = load_eth(&ctx)?;
    if u16::from_be(eth.ether_type_be) != ETH_P_IP {
        return Ok(TC_ACT_PIPE);
    }

    let ip = load_ipv4(&ctx)?;
    if ip.version() != IPV4_PROTOCOL_VERSION || ip.is_fragmented() {
        return Ok(TC_ACT_PIPE);
    }

    let protocol = transport_protocol(ip.protocol)?;
    let l4_offset = ETH_HDR_LEN + ip.header_len_bytes();
    let (src_port_be, dst_port_be, checksum_offset) = load_ports(&ctx, protocol, l4_offset)?;
    let flow_key = FlowStateKey::new(protocol, ip.saddr_be, ip.daddr_be, src_port_be, dst_port_be);
    let flow = unsafe { ETR_TC_FLOW_STATE.get(&flow_key).copied() };
    let flow = match flow {
        Some(flow) => flow,
        None => return Ok(TC_ACT_PIPE),
    };

    rewrite_ipv4_addr(
        &mut ctx,
        ETH_HDR_LEN + offset_of!(Ipv4Hdr, saddr_be),
        ETH_HDR_LEN + offset_of!(Ipv4Hdr, check_be),
        checksum_offset,
        ip.saddr_be,
        flow.rewrite_src_addr_be,
    )?;
    rewrite_l4_port(
        &mut ctx,
        port_offset(protocol, l4_offset, true),
        checksum_offset,
        src_port_be,
        flow.rewrite_src_port_be,
    )?;

    Ok(TC_ACT_PIPE)
}

fn load_eth(ctx: &TcContext) -> Result<EthHdr, i32> {
    ctx.load(0).map_err(|_| TC_ACT_SHOT)
}

fn load_ipv4(ctx: &TcContext) -> Result<Ipv4Hdr, i32> {
    ctx.load(ETH_HDR_LEN).map_err(|_| TC_ACT_SHOT)
}

fn load_ports(
    ctx: &TcContext,
    protocol: TransportProtocol,
    l4_offset: usize,
) -> Result<(u16, u16, usize), i32> {
    match protocol {
        TransportProtocol::Tcp => {
            let tcp: TcpHdr = ctx.load(l4_offset).map_err(|_| TC_ACT_SHOT)?;
            Ok((
                tcp.source_be,
                tcp.dest_be,
                l4_offset + offset_of!(TcpHdr, check_be),
            ))
        }
        TransportProtocol::Udp => {
            let udp: UdpHdr = ctx.load(l4_offset).map_err(|_| TC_ACT_SHOT)?;
            Ok((
                udp.source_be,
                udp.dest_be,
                l4_offset + offset_of!(UdpHdr, check_be),
            ))
        }
    }
}

fn port_offset(protocol: TransportProtocol, l4_offset: usize, source: bool) -> usize {
    match protocol {
        TransportProtocol::Tcp => {
            l4_offset
                + if source {
                    offset_of!(TcpHdr, source_be)
                } else {
                    offset_of!(TcpHdr, dest_be)
                }
        }
        TransportProtocol::Udp => {
            l4_offset
                + if source {
                    offset_of!(UdpHdr, source_be)
                } else {
                    offset_of!(UdpHdr, dest_be)
                }
        }
    }
}

fn rewrite_ipv4_addr(
    ctx: &mut TcContext,
    field_offset: usize,
    ip_checksum_offset: usize,
    l4_checksum_offset: usize,
    old_addr_be: u32,
    new_addr_be: u32,
) -> Result<(), i32> {
    if old_addr_be == new_addr_be {
        return Ok(());
    }

    ctx.l3_csum_replace(
        ip_checksum_offset,
        old_addr_be as u64,
        new_addr_be as u64,
        4,
    )
    .map_err(|_| TC_ACT_SHOT)?;
    ctx.l4_csum_replace(
        l4_checksum_offset,
        old_addr_be as u64,
        new_addr_be as u64,
        L4_IPV4_ADDR_CSUM_FLAGS | BPF_F_MARK_MANGLED_0,
    )
    .map_err(|_| TC_ACT_SHOT)?;
    ctx.store(field_offset, &new_addr_be, 0)
        .map_err(|_| TC_ACT_SHOT)?;
    Ok(())
}

fn rewrite_l4_port(
    ctx: &mut TcContext,
    field_offset: usize,
    checksum_offset: usize,
    old_port_be: u16,
    new_port_be: u16,
) -> Result<(), i32> {
    if old_port_be == new_port_be {
        return Ok(());
    }

    ctx.l4_csum_replace(
        checksum_offset,
        old_port_be as u64,
        new_port_be as u64,
        L4_PORT_CSUM_FLAGS,
    )
    .map_err(|_| TC_ACT_SHOT)?;
    ctx.store(field_offset, &new_port_be, 0)
        .map_err(|_| TC_ACT_SHOT)?;
    Ok(())
}

fn lookup_rule(key: &ForwardRuleKey) -> Option<ForwardRuleValue> {
    unsafe { ETR_TC_FORWARD_RULES.get(key).copied() }
}

fn transport_protocol(protocol: u8) -> Result<TransportProtocol, i32> {
    match protocol {
        IPPROTO_TCP => Ok(TransportProtocol::Tcp),
        IPPROTO_UDP => Ok(TransportProtocol::Udp),
        _ => Err(TC_ACT_PIPE),
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
