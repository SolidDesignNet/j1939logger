use std::{collections::HashMap, mem::swap, sync::Arc};

use canparse::pgn::{ParseMessage, PgnDefinition, PgnLibrary, SpnDefinition};
use rp1210::packet::J1939Packet;
use simple_table::simple_table::{Order, SimpleModel};

pub struct DbcModel {
    // dbc: PgnLibrary,
    pgns: HashMap<u32, PgnDefinition>,
    spns: Vec<SpnDefinition>,
    spn_to_pgn: HashMap<String, u32>,
    get_packet: Box<dyn Fn(u32) -> Option<Arc<J1939Packet>> + Send + Sync>,
}
impl DbcModel {
    pub(crate) fn new(
        dbc: PgnLibrary,
        get_packet_fn: Box<dyn Fn(u32) -> Option<Arc<J1939Packet>> + Send + Sync>,
    ) -> DbcModel {
        let pgns = dbc.pgns;
        let spns = pgns
            .iter()
            .flat_map(|p| p.1.spns.values())
            .map(|s| s.clone())
            .collect();
        let spn_to_pgn: HashMap<String, u32> = pgns
            .iter()
            .flat_map(|p| p.1.spns.values().map(move |s| (s.name.clone(), *p.0)))
            .collect();
        DbcModel {
            pgns,
            spns,
            spn_to_pgn,
            get_packet: get_packet_fn,
        }
    }

    fn spns(&self) -> &Vec<SpnDefinition> {
        &self.spns
    }

    fn pgn_for_spn(&self, spn: &SpnDefinition) -> Option<&PgnDefinition> {
        self.spn_to_pgn
            .get(&spn.name)
            .and_then(|id| self.pgns.get(id))
    }

    fn spn_value(&self, pgn: &PgnDefinition, spn: &SpnDefinition) -> String {
        (self.get_packet)(pgn.pgn_long & 0xFFFFFF).map_or("no packet".to_string(), |packet| {
            spn.parse_message(packet.data())
                .map_or("unable to parse".to_string(), |value| {
                    format!("{} {:.3}", value, spn.units)
                })
        })
    }

    fn packet_string(&self, pgn: &PgnDefinition) -> String {
        (self.get_packet)(pgn.pgn_long & 0xFFFFFF)
            .map_or("no packet".to_string(), |p| p.to_string())
    }
}

impl SimpleModel for DbcModel {
    fn row_count(&mut self) -> usize {
        self.spns().len()
    }

    fn column_count(&mut self) -> usize {
        6
    }

    fn header(&mut self, col: usize) -> String {
        ["ID", "PGN", "SA", "Name", "Value", "Packet"][col].into()
    }

    fn column_width(&mut self, col: usize) -> u32 {
        match col {
            0 => 0,
            1 => 80,
            2 => 40,
            3 => 200,
            4 => 80,
            5 => 400,
            _ => 80,
        }
    }

    fn cell(&mut self, row: i32, col: i32) -> Option<String> {
        let spn = &self.spns[row as usize];
        let pgn = self
            .pgn_for_spn(spn)
            .expect(format!("Missing pgn for spn {:?}", spn).as_str());

        match col {
            0 => Some(format!("{:08X}", pgn.pgn_long)),
            1 => Some(format!("{:04X}", (pgn.pgn_long >> 8) & 0xFFFF)), // FIXME missing 3 bits
            2 => Some(format!("{:02X}", pgn.pgn_long & 0xFF)),
            3 => Some(spn.name.clone().into()),
            4 => Some(self.spn_value(pgn, spn)),
            5 => Some(self.packet_string(pgn)),
            _ => None,
        }
    }

    fn sort(&mut self, col: usize, order: Order) {
        if let Order::None = order {
            return;
        }
        let mut list = vec![];
        swap(&mut list, &mut self.spns);

        list.sort_by(|a, b| {
            let a_pgn = &self.pgn_for_spn(a).expect("Missing pgn a");
            let b_pgn = &self.pgn_for_spn(b).expect("Missing pgn b");
            let v = match col {
                0 => b_pgn.pgn_long.cmp(&a_pgn.pgn_long),
                1 => {
                    let a = &(a_pgn.pgn_long >> 8) & 0xFFFF;
                    let b = &(b_pgn.pgn_long >> 8) & 0xFFFF;
                    b.cmp(&a)
                }
                2 => (b_pgn.pgn_long & 0xFF).cmp(&(a_pgn.pgn_long & 0xFF)),
                3 => b.name.cmp(&a.name),
                4 => self.spn_value(b_pgn, b).cmp(&self.spn_value(a_pgn, a)),
                5 => self.packet_string(b_pgn).cmp(&self.packet_string(a_pgn)),
                _ => std::cmp::Ordering::Equal,
            };
            match order {
                Order::Descending => v.reverse(),
                Order::Ascending => v,
                Order::None => panic!("Should not happen"),
            }
        });
        swap(&mut list, &mut self.spns);
    }
}
