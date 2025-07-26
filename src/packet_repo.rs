use std::{collections::HashMap, time::Duration};

use can_adapter::j1939::j1939_packet::J1939Packet;


#[derive(Clone, Default)]
pub struct PacketRepo {
    packets: Vec<J1939Packet>,
    map: HashMap<u32, Vec<J1939Packet>>,
}

impl PacketRepo {
    pub fn push(&mut self, packet: J1939Packet) {
        self.packets.push(packet.clone());
        self.map
            .entry(packet.id() & (0x3FFFFFF))
            .or_default()
            .push(packet);
    }
    pub fn clear(&mut self) {
        self.packets.clear();
        self.map.clear();
    }
    pub fn get_for(&self, id: u32) -> Option<&Vec<J1939Packet>> {
        self.map.get(&id)
    }
    pub fn last_time(&self) -> Duration {
        self.packets.last().and_then(|p| p.time()).unwrap_or_default()
    }
    pub fn first_time(&self) -> Duration {
        self.packets.first().and_then(|p| p.time()).unwrap_or_default()
    }
    pub fn packets(&self) -> &Vec<J1939Packet> {
        &self.packets
    }
}
