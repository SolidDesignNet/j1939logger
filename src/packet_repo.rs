use std::collections::HashMap;

use can_adapter::packet::J1939Packet;

#[derive(Clone, Default)]
pub struct PacketRepo {
    packets: Vec<J1939Packet>,
    map: HashMap<u32, Vec<J1939Packet>>,
}

impl PacketRepo {
    pub fn push(&mut self, packet: &J1939Packet) {
        self.packets.push(packet.clone());
        self.map
            .entry(packet.id() & (0x3FFFFFF))
            .or_insert_with(|| Vec::new())
            .push(packet.clone());
    }
    pub fn clear(&mut self) {
        self.packets.clear();
        self.map.clear();
    }
    pub fn get_for(&self, id: u32) -> Option<&Vec<J1939Packet>> {
        self.map.get(&id)
    }
    pub fn last_time(&self) -> u32 {
        self.packets.last().map(|p| p.time()).unwrap_or_default()
    }
    pub fn first_time(&self) -> u32 {
        self.packets.first().map(|p| p.time()).unwrap_or_default()
    }
    pub fn packets(&self) -> &Vec<J1939Packet> {
        &self.packets
    }
}
