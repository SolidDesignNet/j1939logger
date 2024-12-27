use std::sync::{Arc, RwLock};

use can_adapter::packet::J1939Packet;
use simple_table::simple_table::{Order, SimpleModel};

/// simple table model to represent log
#[derive(Clone, Default)]
pub struct PacketModel {
    packets: Arc<RwLock<Vec<J1939Packet>>>,
}

impl PacketModel {
    pub fn new(packets: Arc<RwLock<Vec<J1939Packet>>>) -> PacketModel {
        PacketModel { packets }
    }
}

impl SimpleModel for PacketModel {
    fn row_count(&mut self) -> usize {
        self.packets.read().unwrap().len()
    }

    fn column_count(&mut self) -> usize {
        1
    }

    fn header(&mut self, _col: usize) -> String {
        "packet".into()
    }

    fn column_width(&mut self, _col: usize) -> u32 {
        1200
    }

    fn cell(&mut self, row: i32, _col: i32) -> Option<String> {
        self.packets
            .read()
            .unwrap()
            .get(row as usize)
            .map(|p| p.to_string())
    }

    fn sort(&mut self, _col: usize, _order: Order) {
        // sorting not supported
    }
}
