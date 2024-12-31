use std::sync::{Arc, RwLock};

use simple_table::simple_table::{Order, SimpleModel};

use crate::packet_repo::PacketRepo;

/// simple table model to represent log
#[derive(Clone, Default)]
pub struct PacketModel {
    pub packets: Arc<RwLock<PacketRepo>>,
}

impl PacketModel {
    pub fn new(packets: Arc<RwLock<PacketRepo>>) -> PacketModel {
        PacketModel { packets }
    }
}

impl SimpleModel for PacketModel {
    fn row_count(&mut self) -> usize {
        self.packets.read().unwrap().packets.len()
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
            .unwrap().packets
            .get(row as usize)
            .map(|p| p.to_string())
    }

    fn sort(&mut self, _col: usize, _order: Order) {
        // sorting not supported
    }
}
