use etr_config::{EtrConfig, ForwardRule, Protocol, SnatMode};
use etr_types::{ForwardRuleKey, ForwardRuleValue, SnatMode as KernelSnatMode, TransportProtocol};
use std::net::IpAddr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedRule {
    pub name: String,
    pub key: ForwardRuleKey,
    pub value: ForwardRuleValue,
}

pub fn encode_enabled_rules(config: &EtrConfig) -> Vec<EncodedRule> {
    config
        .rules
        .iter()
        .filter(|rule| rule.enabled)
        .map(encode_rule)
        .collect()
}

fn encode_rule(rule: &ForwardRule) -> EncodedRule {
    let backend = &rule.backends[0];
    EncodedRule {
        name: rule.name.clone(),
        key: ForwardRuleKey::new(
            to_transport_protocol(rule.protocol),
            ipv4_to_be_u32(rule.listen_addr),
            rule.listen_port.to_be(),
        ),
        value: ForwardRuleValue::new(
            ipv4_to_be_u32(backend.addr),
            backend.port.to_be(),
            to_kernel_snat_mode(rule.snat),
        ),
    }
}

fn to_transport_protocol(protocol: Protocol) -> TransportProtocol {
    match protocol {
        Protocol::Tcp => TransportProtocol::Tcp,
        Protocol::Udp => TransportProtocol::Udp,
    }
}

fn to_kernel_snat_mode(mode: SnatMode) -> KernelSnatMode {
    match mode {
        SnatMode::Masquerade => KernelSnatMode::Masquerade,
    }
}

fn ipv4_to_be_u32(ip: IpAddr) -> u32 {
    match ip {
        // The TC eBPF program reads IPv4 addresses as raw packet bytes on a
        // little-endian `bpfel` target, so user space must encode map values
        // using native-endian integers whose in-memory bytes match the packet.
        IpAddr::V4(addr) => u32::from_ne_bytes(addr.octets()),
        IpAddr::V6(_) => unreachable!("config validation rejects IPv6 in the MVP"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use etr_config::{
        AddressFamily, BackendTarget, DataPlaneConfig, DataPlaneKind, ManagementConfig,
        ServiceConfig,
    };
    use std::net::{Ipv4Addr, SocketAddr};

    fn sample_config() -> EtrConfig {
        EtrConfig {
            service: ServiceConfig {
                node_name: "edge-hk-01".to_owned(),
            },
            management: ManagementConfig {
                listen: "127.0.0.1:9911".parse::<SocketAddr>().unwrap(),
            },
            data_plane: DataPlaneConfig {
                kind: DataPlaneKind::Tc,
                external_interface: "eth0".to_owned(),
            },
            rules: vec![ForwardRule {
                name: "ssh-proxy".to_owned(),
                protocol: Protocol::Tcp,
                family: AddressFamily::Ipv4,
                listen_addr: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                listen_port: 16020,
                enabled: true,
                snat: SnatMode::Masquerade,
                backends: vec![BackendTarget {
                    addr: IpAddr::V4(Ipv4Addr::new(163, 223, 125, 6)),
                    port: 11426,
                    weight: 1,
                }],
            }],
        }
    }

    #[test]
    fn encodes_enabled_rules_in_network_order() {
        let encoded = encode_enabled_rules(&sample_config());
        assert_eq!(encoded.len(), 1);

        let rule = &encoded[0];
        assert_eq!(rule.key.protocol, TransportProtocol::Tcp as u8);
        assert_eq!(rule.key.listen_addr_be, u32::from_ne_bytes([0, 0, 0, 0]));
        assert_eq!(rule.key.listen_port_be, 16020u16.to_be());
        assert_eq!(
            rule.value.backend_addr_be,
            u32::from_ne_bytes(Ipv4Addr::new(163, 223, 125, 6).octets())
        );
        assert_eq!(rule.value.backend_port_be, 11426u16.to_be());
    }

    #[test]
    fn encodes_non_zero_listen_addr_using_packet_byte_layout() {
        let mut config = sample_config();
        config.rules[0].listen_addr = IpAddr::V4(Ipv4Addr::new(81, 71, 89, 210));

        let encoded = encode_enabled_rules(&config);
        assert_eq!(encoded.len(), 1);
        assert_eq!(
            encoded[0].key.listen_addr_be,
            u32::from_ne_bytes(Ipv4Addr::new(81, 71, 89, 210).octets())
        );
    }
}
