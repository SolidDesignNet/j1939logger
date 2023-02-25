use std::mem::swap;

use canparse::pgn::{ParseMessage, PgnDefinition, PgnLibrary, SpnDefinition};
use rp1210::packet::J1939Packet;
use simple_table::simple_table::{Order, SimpleModel, SparkLine};

pub struct DbcModel {
    pgns: Vec<PgnDefinition>,
    // pgn index in pgns, spn index in pgn
    rows: Vec<(usize, usize)>,
    get_packet: Box<dyn Fn(u32) -> Option<J1939Packet> + Send + Sync>,
    get_packets: Box<dyn Fn(u32) -> Vec<J1939Packet> + Send + Sync>,
}

impl DbcModel {
    pub fn new(
        dbc: PgnLibrary,
        get_packet_fn: Box<dyn Fn(u32) -> Option<J1939Packet> + Send + Sync>,
        get_packets_fn: Box<dyn Fn(u32) -> Vec<J1939Packet> + Send + Sync>,
    ) -> DbcModel {
        let pgns: Vec<PgnDefinition> = dbc.pgns.values().cloned().collect();
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

        DbcModel {
            pgns,
            rows,
            get_packet: get_packet_fn,
            get_packets: get_packets_fn,
        }
    }

    fn spn_value(&self, row: Row) -> String {
        // ignore pritority?
        (self.get_packet)(row.pgn.id & 0x3FFFFFF).map_or("no packet".to_string(), |packet| {
            row.decode(&packet)
                .map_or("unable to parse".to_string(), |value| {
                    format!("{:0.3} {}", value, row.spn.units)
                })
        })
    }

    fn packet_string(&self, pgn: &PgnDefinition) -> String {
        // ignore priority?
        (self.get_packet)(pgn.id & 0x3FFFFFF).map_or("no packet".to_string(), |p| p.to_string())
    }

    fn lookup_row(&self, index: &(usize, usize)) -> Row {
        let pgn = &self.pgns[index.0];
        let spns: Vec<&SpnDefinition> = pgn.spns.values().collect();
        Row {
            pgn,
            spn: spns[index.1],
        }
    }
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
            1 => 80,
            2 => 40,
            3 => 200,
            4 => 80,
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

    fn cell_delegate(
        &mut self,
        row: i32,
        col: i32,
    ) -> Option<Box<dyn simple_table::simple_table::DrawDelegate>> {
        if col == 5 {
            let row = self.lookup_row(&self.rows[row as usize]);
            let data = (self.get_packets)(row.pgn.id)
                .iter()
                .map(|p| row.decode(p).unwrap_or(0.0))
                .collect();
            Some(Box::new(SparkLine::new(data)))
        } else {
            None
        }
    }
}
struct Row<'a> {
    spn: &'a SpnDefinition,
    pgn: &'a PgnDefinition,
}
impl Row<'_> {
    fn decode(&self, packet: &J1939Packet) -> Option<f32> {
        self.spn.parse_message(packet.data())
    }
}
