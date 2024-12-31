use std::collections::HashMap;

use can_adapter::packet::J1939Packet;

#[derive(Clone, Default)]
pub struct PacketRepo {
    pub packets: Vec<J1939Packet>,
    pub map: HashMap<u32, Vec<J1939Packet>>,
}

impl PacketRepo {
    pub fn push(&mut self, packet: &J1939Packet) {
        self.packets.push(packet.clone());
        self.map.entry(packet.id()&(0x3FFFFFF)).or_insert_with(||Vec::new()).push(packet.clone() );
    }
    pub fn clear(&mut self) {
        self.packets.clear();
    self.map.clear();
    }
}
