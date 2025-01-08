use core::f64;
use std::{
    mem::swap,
    sync::{Arc, RwLock},
};

use can_adapter::packet::J1939Packet;
use canparse::pgn::{ParseMessage, PgnDefinition, SpnDefinition};
use fltk::utils::decode_uri;
use simple_table::simple_table::{DrawDelegate, Order, SimpleModel, SparkLine};

use crate::packet_repo::PacketRepo;

/// SimpleModel representing a DBC file with a Connection.
pub struct DbcModel {
    /// PGN Definitions in row order.
    pgns: Vec<PgnDefinition>,
    /// pgn -> packets in chronological order
    packets: Arc<RwLock<PacketRepo>>,
    /// meat of the struct
    rows: Vec<Row>,
    /// Most recent packet before this instant will be used.
    /// packet time, not wallclock!
    time: u32,
}
impl DbcModel {
    pub fn new(pgns: Vec<PgnDefinition>, packets: Arc<RwLock<PacketRepo>>) -> DbcModel {
        let mut m = DbcModel {
            pgns,
            rows: Vec::new(),
            packets,
            time: u32::MAX,
        };
        m.restore_missing();
        m
    }
    pub fn set_time(&mut self, t: u32) {
        self.time = t;
    }
    pub fn remove_missing(self: &mut Self) {
        let new_rows = self
            .rows
            .iter()
            .filter(|row| {
                self.packets
                    .read()
                    .unwrap()
                    .get_for(row.pgn.id & 0x3FFFFFF)
                    .is_some()
            })
            .cloned()
            .collect();
        self.rows = new_rows;
    }
    pub fn restore_missing(self: &mut Self) {
        self.rows = calc_rows(&self.pgns);
    }

    fn spn_value(&self, row: &Row) -> String {
        // ignore pritority?
        self.last_packet(row.pgn.id & 0x3FFFFFF)
            .map_or("no packet".to_string(), |packet| {
                row.decode(&packet)
                    .map_or("unable to parse".to_string(), |value| {
                        format!("{:0.3} {}", value, row.spn.units)
                    })
            })
    }

    fn packet_string(&self, pgn: &PgnDefinition) -> String {
        // ignore priority?
        self.last_packet(pgn.id & 0x3FFFFFF)
            .map_or("no packet".to_string(), |p| p.to_string())
    }

    fn last_packet(&self, id: u32) -> Option<J1939Packet> {
        return self.packets.read().unwrap().get_for(id).map_or(None, |v| {
            // FIXME replace with partition.  It will do a binary search.
            v.iter().rev().find(|p| p.time() <= self.time).cloned()
        });
    }
    pub fn map_address(&mut self, from: u8, to: u8) {
        let f = from as u32;
        let t = to as u32;
        self.pgns = self
            .pgns
            .iter()
            .map(|pgn_def| {
                let k = pgn_def.id;
                let mut pgn_definition = pgn_def.clone();
                if (0xFF & k) == f {
                    pgn_definition.id = 0xFFFFFF00 & k | t;
                }
                pgn_definition
            })
            .collect();
    }

    pub(crate) fn toggle_missing(&mut self) {
        if calc_rows(&self.pgns).len() == self.rows.len() {
            self.remove_missing();
        } else {
            self.rows = calc_rows(&self.pgns);
        }
    }
}

fn calc_rows<'a>(pgns: &'a Vec<PgnDefinition>) -> Vec<Row> {
    pgns.iter()
        .flat_map(|p| {
            p.spns.values().map(|s| Row {
                spn: s.clone(),
                pgn: p.clone(),
            })
        })
        .collect()
}

impl SimpleModel for DbcModel {
    fn row_count(&mut self) -> usize {
        self.rows.len()
    }

    fn column_count(&mut self) -> usize {
        7
    }

    fn header(&mut self, col: usize) -> String {
        ["ID", "PGN", "SA", "Name", "Value", "Chart", "Packet"][col].into()
    }

    fn column_width(&mut self, col: usize) -> u32 {
        match col {
            0 => 0,
            1 => 40,
            2 => 40,
            3 => 300,
            4 => 160,
            5 => 120,
            6 => 400,
            _ => 80,
        }
    }

    fn cell(&mut self, row: i32, col: i32) -> Option<String> {
        let row = self.rows.get(row as usize).expect("Unknown row requested");

        match col {
            0 => Some(format!("{:08X}", row.pgn.id)),
            1 => Some(format!("{:04X}", row.pgn.pgn())), // FIXME missing 3 bits
            2 => Some(format!("{:02X}", row.pgn.sa())),
            3 => Some(row.spn.name.clone().into()),
            4 => Some(self.spn_value(row)),
            6 => Some(self.packet_string(&row.pgn)),
            _ => None,
        }
    }

    fn cell_delegate(&mut self, row: i32, col: i32) -> Option<Box<dyn DrawDelegate>> {
        match col {
            5 => {
                let row = self.rows.get(row as usize).expect("Unknown row requested");
                let id = row.pgn.id & 0x3FFFFFF;
                let repo = self.packets.read().unwrap();
                let packets = repo.get_for(id);
                if packets.is_none() || packets.unwrap().is_empty() {
                    // requires some packets to calculate time range.
                    return None;
                }
                let packets = packets.unwrap();
                let end = u32::min(repo.last_time(), self.time);
                let time_stamp_weight = packets.get(0).unwrap().time_stamp_weight();
                let start = end as i32 - (10.0 * time_stamp_weight) as i32;
                let start = if start < 0 { 0 } else { start } as u32;

                dbg!(start, end);

                let start_index = packets.partition_point(|p| p.time() < start);
                let end_index = packets.partition_point(|p| p.time() < end);
                let data = packets[start_index..end_index]
                    .iter()
                    .filter_map(|p| row.decode(p))
                    .collect();
                Some(Box::new(SparkLine::new(data)) as Box<dyn DrawDelegate>)
            }
            _ => None,
        }
    }

    fn sort(&mut self, col: usize, order: Order) {
        if let Order::None = order {
            return;
        }
        let mut list = vec![];
        swap(&mut list, &mut self.rows);
        list.sort_by(|a, b| {
            let o = match col {
                0 => b.pgn.id.cmp(&a.pgn.id),
                1 => b.pgn.pgn().cmp(&a.pgn.pgn()),
                2 => b.pgn.sa().cmp(&a.pgn.sa()),
                3 => b.spn.name.cmp(&a.spn.name),
                4 => self.spn_value(b).cmp(&self.spn_value(a)),
                5 => self.spn_value(b).cmp(&self.spn_value(a)),
                6 => self.packet_string(&b.pgn).cmp(&self.packet_string(&a.pgn)),
                _ => panic!("unknown column"),
            };
            order.apply(o)
        });
        swap(&mut list, &mut self.rows);
    }
}
#[derive(Clone)]
struct Row {
    spn: SpnDefinition,
    pgn: PgnDefinition,
}
impl Row {
    fn decode(&self, packet: &J1939Packet) -> Option<f64> {
        self.spn.parse_message(packet.data()).map(|v| v as f64)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn verify_bin_search() {
        let v = vec![1., 2., 3., 4., 5.];
        assert_eq!(v.partition_point(|&x| x <= 2.5), 2);
        assert_eq!(v.partition_point(|&x| x <= 3.0), 3);
        assert_eq!(v.partition_point(|&x| x <= 3.5), 3);
    }
}
