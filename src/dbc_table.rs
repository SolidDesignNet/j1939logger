use core::f64;
use std::{
    collections::HashMap,
    fmt::Debug,
    hash::Hash,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use can_adapter::packet::Packet;
use canparse::pgn::{ParseMessage, PgnDefinition, SpnDefinition};
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
    time: Duration,
    // how long should the spark line be
    line_length: Duration,
}
impl DbcModel {
    pub fn new(pgns: Vec<PgnDefinition>, packets: Arc<RwLock<PacketRepo>>) -> DbcModel {
        let mut m = DbcModel {
            pgns,
            rows: Vec::new(),
            packets,
            time: Duration::MAX,
            line_length: Duration::from_secs(10),
        };
        m.restore_missing();
        m
    }
    pub fn set_time(&mut self, t: Duration) {
        self.time = t;
    }
    pub fn remove_missing(&mut self) {
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
    pub fn restore_missing(&mut self) {
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

    fn last_packet(&self, id: u32) -> Option<Packet> {
        return self.packets.read().unwrap().get_for(id).and_then(|v| {
            // FIXME replace with partition.  It will do a binary search.
            v.iter()
                .rev()
                .find(|p| p.time().unwrap_or_default() <= self.time)
                .map(|p| p.into())
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

    pub fn set_line_length(&mut self, line_length: Duration) {
        self.line_length = line_length;
    }
}

fn calc_rows(pgns: &[PgnDefinition]) -> Vec<Row> {
    pgns.iter()
        .flat_map(|p| {
            p.spns.values().map(|s| Row {
                spn: s.clone(),
                pgn: p.clone(),
            })
        })
        .collect()
}

const COLUMNS: [&str; 8] = [
    "ID",
    "PGN",
    "SA",
    "Name",
    "Value",
    "Chart",
    "Packet",
    "Description",
];
impl SimpleModel for DbcModel {
    fn row_count(&mut self) -> usize {
        self.rows.len()
    }

    fn column_count(&mut self) -> usize {
        COLUMNS.len()
    }

    fn header(&mut self, col: usize) -> String {
        COLUMNS[col].into()
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
            7 => 150,
            _ => 80,
        }
    }

    fn cell(&mut self, row: i32, col: i32) -> Option<String> {
        let row = self.rows.get(row as usize).expect("Unknown row requested");

        match col {
            0 => Some(format!("{:08X}", row.pgn.id)),
            1 => Some(format!("{:04X}", row.pgn.pgn())),
            2 => Some(format!("{:02X}", row.pgn.sa())),
            3 => Some(row.spn.name.clone()),
            4 => Some(self.spn_value(row)),
            6 => Some(self.packet_string(&row.pgn)),
            7 => Some(row.spn.description.clone()),
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
                let end = Duration::min(repo.last_time(), self.time);
                let start = if end > self.line_length {
                    end - self.line_length
                } else {
                    Duration::default()
                };

                let start_index = packets.partition_point(|p| p.time().unwrap_or_default() < start);
                let end_index = packets.partition_point(|p| p.time().unwrap_or_default() < end);

                let data = packets[start_index..end_index]
                    .iter()
                    .filter_map(|p| row.decode(p))
                    .collect();
                Some(Box::new(SparkLine::new(data)) as Box<dyn DrawDelegate>)
            }
            _ => None,
        }
    }

    fn sort<'a>(&mut self, column: usize, order: Order) {
        if let Order::None = order {
            return;
        }

        self.rows = match column {
            0 => sort_with(&self.rows, |row: &Row| row.pgn.id),
            1 => sort_with(&self.rows, |row: &Row| row.pgn.pgn()),
            2 => sort_with(&self.rows, |row: &Row| row.pgn.sa()),
            3 => sort_with(&self.rows, |row: &Row| row.spn.name.clone()),
            4 | 5 => sort_with(&self.rows, |row| self.spn_value(row)),
            6 => sort_with(&self.rows, |row: &Row| self.packet_string(&row.pgn)),
            7 => sort_with(&self.rows, |row: &Row| row.spn.description.clone()),
            _ => panic!("unknown column"),
        };
        match order {
            Order::Descending => self.rows.reverse(),
            Order::Ascending | Order::None => (),
        }
    }
}

fn sort_with<T: Ord>(the_rows: &[Row], extract_fn: impl Fn(&Row) -> T) -> Vec<Row> {
    let values: HashMap<&Row, T> = the_rows.iter().map(|row| (row, extract_fn(row))).collect();
    let mut rows: Vec<Row> = the_rows.into();
    rows.sort_by(|a: &Row, b: &Row| values.get(a).cmp(&values.get(b)));
    rows
}

#[derive(Clone, Debug)]
struct Row {
    spn: SpnDefinition,
    pgn: PgnDefinition,
}
impl Row {
    fn decode(&self, packet: &Packet) -> Option<f64> {
        self.spn
            .parse_message(packet.payload.as_slice())
            .map(|v| v as f64)
    }
}
impl Hash for Row {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.spn.name.hash(state);
        self.spn.id.hash(state);
        self.pgn.id.hash(state);
    }
}
impl Eq for Row {}
impl PartialEq for Row {
    fn eq(&self, other: &Self) -> bool {
        self.spn == other.spn && self.pgn == other.pgn
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn verify_bin_search() {
        let v = [1., 2., 3., 4., 5.];
        assert_eq!(v.partition_point(|&x| x <= 2.5), 2);
        assert_eq!(v.partition_point(|&x| x <= 3.0), 3);
        assert_eq!(v.partition_point(|&x| x <= 3.5), 3);
    }
}
