use std::{
    collections::{HashMap, VecDeque},
    mem::swap,
    sync::{Arc, RwLock},
};

use canparse::pgn::{ParseMessage, PgnDefinition, PgnLibrary, SpnDefinition};
use rp1210::packet::J1939Packet;
use simple_table::simple_table::{DrawDelegate, Order, SimpleModel, SparkLine};

#[derive(Debug, Clone)]
pub struct DbcModel {
    pgns: Vec<PgnDefinition>,
    // pgn index in pgns, spn index in pgn
    rows: Vec<(usize, usize)>,
    packets: Arc<RwLock<HashMap<u32, VecDeque<J1939Packet>>>>,
}
impl DbcModel {
    pub fn new(
        dbc: PgnLibrary,
        packets: Arc<RwLock<HashMap<u32, VecDeque<J1939Packet>>>>,
    ) -> DbcModel {
        new_with_pgns(dbc.pgns.values().cloned().collect(), packets)
    }

    pub fn remove_missing(self: &mut Self) {
        let lock = self.packets.read();
        if let Ok(map) = lock {
            self.rows = self
                .rows
                .iter()
                .filter(|(i, _)| {
                    let def = self.pgns.get(*i);
                    if let Some(pd) = def {
                        map.contains_key(&(0xFFFF_FF & pd.id))
                    } else {
                        false
                    }
                })
                .cloned()
                .collect();
        }
    }
    pub fn restore_missing(self: &mut Self) {
        self.rows = calc_rows(&self.pgns);
    }

    fn spn_value(&self, row: Row) -> String {
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

    fn lookup_row(&self, index: &(usize, usize)) -> Row {
        let pgn = &self.pgns[index.0];
        let spns: Vec<&SpnDefinition> = pgn.spns.values().collect();
        Row {
            pgn,
            spn: spns[index.1],
        }
    }

    fn last_packet(&self, id: u32) -> Option<J1939Packet> {
        self.packets
            .read()
            .unwrap()
            .get(&id)
            .and_then(|v| v.back())
            .cloned()
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
        if calc_rows(&self.pgns).len() == self.row_count() {
            self.remove_missing()
        } else {
            self.restore_missing()
        }
    }
}

pub fn new_with_pgns(
    pgns: Vec<PgnDefinition>,
    packets: Arc<RwLock<HashMap<u32, VecDeque<J1939Packet>>>>,
) -> DbcModel {
    let rows = calc_rows(&pgns);

    DbcModel {
        pgns,
        rows,
        packets,
    }
}

fn calc_rows(pgns: &Vec<PgnDefinition>) -> Vec<(usize, usize)> {
    let mut rows = Vec::new();

    let mut p = 0;
    while p < pgns.len() {
        let mut s = 0;
        while s < pgns[p].spns.len() {
            rows.push((p, s));
            s = s + 1;
        }
        p = p + 1;
    }
    rows
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
        let row = self.lookup_row(&self.rows[row as usize]);

        match col {
            0 => Some(format!("{:08X}", row.pgn.id)),
            1 => Some(format!("{:04X}", row.pgn.pgn())), // FIXME missing 3 bits
            2 => Some(format!("{:02X}", row.pgn.sa())),
            3 => Some(row.spn.name.clone().into()),
            4 => Some(self.spn_value(row)),
            6 => Some(self.packet_string(row.pgn)),
            _ => None,
        }
    }

    fn cell_delegate(&mut self, row: i32, col: i32) -> Option<Box<dyn DrawDelegate>> {
        match col {
            5 => {
                let row = self.lookup_row(&self.rows[row as usize]);
                let id = row.pgn.id & 0x3FFFFFF;
                self.packets
                    .read()
                    .unwrap()
                    .get(&id)
                    .map(|v| v.iter().map(|p| row.decode(p).unwrap_or(0.0)).collect())
                    .map(|data: Vec<f64>| Box::new(SparkLine::new(data)) as Box<dyn DrawDelegate>)
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
            let a = self.lookup_row(a);
            let b = self.lookup_row(b);
            let o = match col {
                0 => b.pgn.id.cmp(&a.pgn.id),
                1 => b.pgn.pgn().cmp(&a.pgn.pgn()),
                2 => b.pgn.sa().cmp(&a.pgn.sa()),
                3 => b.spn.name.cmp(&a.spn.name),
                4 => self.spn_value(b).cmp(&self.spn_value(a)),
                5 => self.spn_value(b).cmp(&self.spn_value(a)),
                6 => self.packet_string(b.pgn).cmp(&self.packet_string(a.pgn)),
                _ => panic!("unknown column"),
            };
            order.apply(o)
        });
        swap(&mut list, &mut self.rows);
    }
}
struct Row<'a> {
    spn: &'a SpnDefinition,
    pgn: &'a PgnDefinition,
}
impl Row<'_> {
    fn decode(&self, packet: &J1939Packet) -> Option<f64> {
        self.spn.parse_message(packet.data()).map(|v| v as f64)
    }
}
