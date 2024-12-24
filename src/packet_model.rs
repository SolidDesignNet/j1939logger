use std::{
    collections::{HashMap, VecDeque},
    sync::{atomic::AtomicBool, Arc, RwLock},
    thread,
    time::{Duration, Instant},
};

use can_adapter::{common::Connection, packet::J1939Packet};
use simple_table::simple_table::{Order, SimpleModel};

/// simple table model to represent log
#[derive(Clone, Default)]
pub struct PacketModel {
    pub list: Arc<RwLock<Vec<J1939Packet>>>,
    pub table: Arc<RwLock<HashMap<u32, VecDeque<J1939Packet>>>>,
    running: Arc<AtomicBool>,
}

impl PacketModel {
    // FIXME don't we have leaked threads?
    pub fn stop(&mut self) {
        // stop previous
        self.running.store(false, std::sync::atomic::Ordering::Relaxed);
    }
    /// copy packets from bus to table
    pub fn run(&mut self, bus: &mut dyn Connection) {
        let list = self.list.clone();
        let table = self.table.clone();
        let iter_for = bus.iter_for(Duration::from_secs(60 * 60 * 24 * 7));

        self.running=Arc::new(AtomicBool::new(true));
        let running = self.running.clone();
        Some(thread::spawn(move || {
            let mut last_trim = Instant::now();
            for p in iter_for {
                if !running.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                let start = p.time() - 15.0; // 15 s
                list.write().unwrap().push(p.clone());
                let mut hash_map = table.write().unwrap();
                if let Some(v) = hash_map.get_mut(&p.id()) {
                    v.push_back(p);
                } else {
                    let id = p.id();
                    let mut vd = VecDeque::new();
                    vd.push_back(p);
                    hash_map.insert(id, vd);
                }
                // clean up every 200 ms
                if last_trim.elapsed() > Duration::from_millis(200) {
                    hash_map.values_mut().for_each(|v| {
                        while v.front().map_or(false, |p| p.time() < start) {
                            v.pop_front();
                        }
                    });
                    last_trim = Instant::now();
                }
            }
        }));
    }
}

impl SimpleModel for PacketModel {
    fn row_count(&mut self) -> usize {
        self.list.read().unwrap().len()
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
        self.list
            .read()
            .unwrap()
            .get(row as usize)
            .map(|p| p.to_string())
    }

    fn sort(&mut self, _col: usize, _order: Order) {
        // sorting not supported
    }
}
