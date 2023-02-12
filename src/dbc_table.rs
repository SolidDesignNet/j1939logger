use std::sync::Arc;

use canparse::pgn::{ParseMessage, PgnDefinition, PgnLibrary, SpnDefinition};
use rp1210::packet::J1939Packet;
use simple_table::simple_table::SimpleModel;

pub struct DbcModel {
    dbc: PgnLibrary,
    get_packet: Box<dyn Fn(u32) -> Option<Arc<J1939Packet>> + Send + Sync>,
}
impl DbcModel {
    pub(crate) fn new(
        dbc: PgnLibrary,
        get_packet_fn: Box<dyn Fn(u32) -> Option<Arc<J1939Packet>> + Send + Sync>,
    ) -> DbcModel {
        DbcModel {
            dbc,
            get_packet: get_packet_fn,
        }
    }
    pub fn pgns(&self) -> Vec<&PgnDefinition> {
        self.dbc.pgns.values().collect()
    }
    pub fn spns(&self) -> Vec<&SpnDefinition> {
        self.dbc
            .pgns
            .values()
            .flat_map(|p| p.spns.values())
            .collect()
    }
    pub fn pgns_spns(&self) -> Vec<(&PgnDefinition, &SpnDefinition)> {
        self.dbc
            .pgns
            .values()
            .flat_map(|p| p.spns.values().map(move |s| (p, s)))
            .collect()
    }
}
impl SimpleModel for DbcModel {
    fn row_count(&mut self) -> usize {
        self.spns().len()
    }

    fn column_count(&mut self) -> usize {
        2
    }

    fn header(&mut self, col: usize) -> String {
        match col {
            0 => "name".into(),
            1 => "value".into(),
            _ => "Unknown".into(),
        }
    }

    fn column_width(&mut self, col: usize) -> u32 {
        120
    }

    fn cell(&mut self, row: i32, col: i32) -> Option<String> {
        let (pgn, spn) = self.pgns_spns()[row as usize];

        match col {
            0 => Some(spn.name.clone().into()),
            1 => Some(
                (self.get_packet)(pgn.pgn_long).map_or("no packet".to_string(), |packet| {
                    spn.parse_message(packet.data())
                        .map_or("unable to parse".to_string(), |value| {
                            format!("{} {:.3}", value, "unit")
                        })
                }),
            ),

            _ => None,
        }
    }
}
