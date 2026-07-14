use crate::config;
use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::{IpAddress, IpEndpoint, Ipv4Address, Stack};
use embassy_time::{Duration, Timer};
use esp_println::println;

const SERVER_PORT: u16 = 67;
const CLIENT_PORT: u16 = 68;
const DHCP_MIN_LEN: usize = 240;
const DHCP_MAGIC: [u8; 4] = [99, 130, 83, 99];

const BOOTREQUEST: u8 = 1;
const BOOTREPLY: u8 = 2;
const ETHERNET_HTYPE: u8 = 1;
const ETHERNET_HLEN: u8 = 6;

const OPT_SUBNET_MASK: u8 = 1;
const OPT_ROUTER: u8 = 3;
const OPT_DNS: u8 = 6;
const OPT_REQUESTED_IP: u8 = 50;
const OPT_LEASE_TIME: u8 = 51;
const OPT_MESSAGE_TYPE: u8 = 53;
const OPT_SERVER_ID: u8 = 54;
const OPT_RENEWAL_TIME: u8 = 58;
const OPT_REBINDING_TIME: u8 = 59;
const OPT_END: u8 = 255;

const MSG_DISCOVER: u8 = 1;
const MSG_OFFER: u8 = 2;
const MSG_REQUEST: u8 = 3;
const MSG_ACK: u8 = 5;

pub fn init() {
    let _ = (config::DHCP_POOL_START, config::DHCP_POOL_END);
}

#[embassy_executor::task]
pub async fn run(stack: Stack<'static>) {
    stack.wait_config_up().await;

    let mut rx_meta = [PacketMetadata::EMPTY; 2];
    let mut tx_meta = [PacketMetadata::EMPTY; 2];
    let mut rx_buffer = [0; 1200];
    let mut tx_buffer = [0; 1200];
    let mut socket = UdpSocket::new(
        stack,
        &mut rx_meta,
        &mut rx_buffer,
        &mut tx_meta,
        &mut tx_buffer,
    );

    if socket.bind(SERVER_PORT).is_err() {
        println!("DHCP server failed to bind UDP port {}", SERVER_PORT);
        return;
    }

    println!(
        "DHCP server ready: {}.{}.{}.{}-{}.{}.{}.{}",
        config::AP_IP_OCTETS[0],
        config::AP_IP_OCTETS[1],
        config::AP_IP_OCTETS[2],
        config::DHCP_POOL_START,
        config::AP_IP_OCTETS[0],
        config::AP_IP_OCTETS[1],
        config::AP_IP_OCTETS[2],
        config::DHCP_POOL_END
    );

    loop {
        let mut request = [0; 768];
        let Ok((len, _remote)) = socket.recv_from(&mut request).await else {
            Timer::after(Duration::from_millis(50)).await;
            continue;
        };

        let Some(dhcp_request) = DhcpRequest::parse(&request[..len]) else {
            continue;
        };

        let reply_type = match dhcp_request.message_type {
            MSG_DISCOVER => MSG_OFFER,
            MSG_REQUEST => MSG_ACK,
            _ => continue,
        };
        let lease = dhcp_request
            .requested_ip
            .filter(|ip| is_assignable_client_address(*ip))
            .unwrap_or_else(|| lease_for_mac(dhcp_request.client_mac));

        let mut response = [0; 576];
        let response_len = write_reply(&mut response, &dhcp_request, reply_type, lease);
        let destination = IpEndpoint::new(IpAddress::Ipv4(Ipv4Address::BROADCAST), CLIENT_PORT);

        if let Err(error) = socket.send_to(&response[..response_len], destination).await {
            println!("DHCP reply send failed: {:?}", error);
        } else {
            println!(
                "DHCP {} {}.{}.{}.{} for {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                if reply_type == MSG_OFFER {
                    "offer"
                } else {
                    "ack"
                },
                lease[0],
                lease[1],
                lease[2],
                lease[3],
                dhcp_request.client_mac[0],
                dhcp_request.client_mac[1],
                dhcp_request.client_mac[2],
                dhcp_request.client_mac[3],
                dhcp_request.client_mac[4],
                dhcp_request.client_mac[5]
            );
        }
    }
}

struct DhcpRequest {
    xid: [u8; 4],
    flags: [u8; 2],
    client_mac: [u8; 6],
    message_type: u8,
    requested_ip: Option<[u8; 4]>,
}

impl DhcpRequest {
    fn parse(packet: &[u8]) -> Option<Self> {
        if packet.len() < DHCP_MIN_LEN
            || packet[0] != BOOTREQUEST
            || packet[1] != ETHERNET_HTYPE
            || packet[2] != ETHERNET_HLEN
            || packet[236..240] != DHCP_MAGIC
        {
            return None;
        }

        let mut message_type = None;
        let mut requested_ip = None;
        let mut options = &packet[240..];

        while let Some((&code, rest)) = options.split_first() {
            if code == OPT_END {
                break;
            }
            if code == 0 {
                options = rest;
                continue;
            }
            let Some((&option_len, option_data)) = rest.split_first() else {
                break;
            };
            let option_len = option_len as usize;
            if option_data.len() < option_len {
                break;
            }
            let value = &option_data[..option_len];

            match code {
                OPT_MESSAGE_TYPE if option_len == 1 => message_type = Some(value[0]),
                OPT_REQUESTED_IP if option_len == 4 => {
                    requested_ip = Some([value[0], value[1], value[2], value[3]]);
                }
                _ => {}
            }

            options = &option_data[option_len..];
        }

        Some(Self {
            xid: packet[4..8].try_into().ok()?,
            flags: packet[10..12].try_into().ok()?,
            client_mac: packet[28..34].try_into().ok()?,
            message_type: message_type?,
            requested_ip,
        })
    }
}

fn write_reply(
    response: &mut [u8; 576],
    request: &DhcpRequest,
    message_type: u8,
    lease: [u8; 4],
) -> usize {
    response.fill(0);
    response[0] = BOOTREPLY;
    response[1] = ETHERNET_HTYPE;
    response[2] = ETHERNET_HLEN;
    response[4..8].copy_from_slice(&request.xid);
    response[10..12].copy_from_slice(&request.flags);
    response[16..20].copy_from_slice(&lease);
    response[20..24].copy_from_slice(&config::AP_IP_OCTETS);
    response[28..34].copy_from_slice(&request.client_mac);
    response[236..240].copy_from_slice(&DHCP_MAGIC);

    let mut cursor = 240;
    push_option(response, &mut cursor, OPT_MESSAGE_TYPE, &[message_type]);
    push_option(response, &mut cursor, OPT_SERVER_ID, &config::AP_IP_OCTETS);
    push_option(response, &mut cursor, OPT_SUBNET_MASK, &[255, 255, 255, 0]);
    push_option(response, &mut cursor, OPT_ROUTER, &config::AP_IP_OCTETS);
    push_option(response, &mut cursor, OPT_DNS, &config::AP_IP_OCTETS);
    push_option(
        response,
        &mut cursor,
        OPT_LEASE_TIME,
        &config::DHCP_LEASE_SECONDS.to_be_bytes(),
    );
    push_option(
        response,
        &mut cursor,
        OPT_RENEWAL_TIME,
        &(config::DHCP_LEASE_SECONDS / 2).to_be_bytes(),
    );
    push_option(
        response,
        &mut cursor,
        OPT_REBINDING_TIME,
        &((config::DHCP_LEASE_SECONDS * 7) / 8).to_be_bytes(),
    );
    response[cursor] = OPT_END;
    cursor + 1
}

fn push_option(response: &mut [u8], cursor: &mut usize, code: u8, value: &[u8]) {
    response[*cursor] = code;
    response[*cursor + 1] = value.len() as u8;
    *cursor += 2;
    response[*cursor..*cursor + value.len()].copy_from_slice(value);
    *cursor += value.len();
}

fn lease_for_mac(mac: [u8; 6]) -> [u8; 4] {
    let pool_size = config::DHCP_POOL_END - config::DHCP_POOL_START + 1;
    let host = config::DHCP_POOL_START + (mac[5] % pool_size);
    [
        config::AP_IP_OCTETS[0],
        config::AP_IP_OCTETS[1],
        config::AP_IP_OCTETS[2],
        host,
    ]
}

fn is_assignable_client_address(ip: [u8; 4]) -> bool {
    ip[0] == config::AP_IP_OCTETS[0]
        && ip[1] == config::AP_IP_OCTETS[1]
        && ip[2] == config::AP_IP_OCTETS[2]
        && ip[3] != 0
        && ip[3] != config::AP_IP_OCTETS[3]
        && ip[3] != 255
}
