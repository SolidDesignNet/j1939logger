use core::f64;
use std::{
    cell::RefCell,
    collections::HashSet,
    mem::swap,
    sync::{Arc, RwLock},
};

use can_adapter::packet::J1939Packet;
use canparse::pgn::{ParseMessage, PgnDefinition, SpnDefinition};
use simple_table::simple_table::{DrawDelegate, Order, SimpleModel, SparkLine};

/// SimpleModel representing a DBC file with a Connection.
pub struct DbcModel {
    /// PGN Definitions in row order.
    pgns: Vec<PgnDefinition>,
    /// pgn -> packets in chronological order
    packets: Arc<RwLock<Vec<J1939Packet>>>,
    pgns_seen: RefCell<HashSet<u32>>,
    /// meat of the struct
    rows: Vec<Row>,
    /// Most recent packet before this instant will be used.
    /// packet time, not wallclock!
    time: f64,
}
impl DbcModel {
    pub fn new(pgns: Vec<PgnDefinition>, packets: Arc<RwLock<Vec<J1939Packet>>>) -> DbcModel {
        let mut m = DbcModel {
            pgns,
            pgns_seen: RefCell::new(HashSet::new()),
            rows: Vec::new(),
            packets,
            time: f64::MAX,
        };
        m.restore_missing();
        m
    }

    pub fn remove_missing(self: &mut Self) {
        let new_rows = self
            .rows
            .iter()
            .filter(|row| self.pgns_seen.borrow().contains(&(row.pgn.id& 0x3FFFFFF)))
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
        let mut seen = self.pgns_seen.borrow_mut();
        if seen.is_empty() || seen.contains(&id) {
            let p = self
                .packets
                .read()
                .unwrap()
                .iter()
                .rev()
                .find(|p| {
                    seen.insert(p.id());
                    p.time() <= self.time && p.id() == id
                })
                .cloned();
            p
        } else {
            None
        }
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
                if self.pgns_seen.borrow().contains(&id) {
                    let packets = self.packets.read().unwrap();
                    let time = packets
                        .iter()
                        .rev()
                        .next()
                        .map(|p| p.time())
                        .unwrap_or_default();
                    let data = packets
                        .iter()
                        .rev()
                        .filter(|p| p.id() == id)
                        .map_while(|p| {
                            if p.time() > time - 30.0 {
                                row.decode(p)
                            } else {
                                None
                            }
                        })
                        .collect();
                    Some(Box::new(SparkLine::new(data)) as Box<dyn DrawDelegate>)
                } else {
                    None
                }
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
