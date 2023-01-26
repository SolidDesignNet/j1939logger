use std::{
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use clap::Parser;
use fltk::{
    app, enums,
    prelude::{GroupExt, WidgetExt},
    window::Window,
};
use rp1210::{multiqueue::MultiQueue, packet::J1939Packet, ConnectionDescriptor};
use simple_table::simple_table::{SimpleModel, SimpleTable};
use timer::Timer;

#[derive(Default)]
struct PacketModel {
    pub list: Arc<Mutex<Vec<J1939Packet>>>,
}
impl PacketModel {
    pub fn run(&self, bus: MultiQueue<J1939Packet>) -> thread::JoinHandle<()> {
        let list = self.list.clone();
        thread::spawn(move || {
            bus.iter_for(Duration::from_secs(60 * 60 * 24 * 7))
                .for_each(|p| list.lock().unwrap().push(p))
        })
    }
}
impl SimpleModel for PacketModel {
    fn row_count(&mut self) -> usize {
        self.list.lock().unwrap().len()
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
            .lock()
            .unwrap()
            .get(row as usize)
            .map(|p| p.to_string())
    }
}

fn main() -> Result<(), anyhow::Error> {
    let bus: MultiQueue<J1939Packet> = MultiQueue::new();
    let mut rp1210 = ConnectionDescriptor::parse().connect(bus.clone())?;
    rp1210.run();

    let packet_model = PacketModel::default();
    packet_model.run(bus.clone());

    let app = app::App::default();
    let mut wind = Window::default().with_size(400, 600).with_label("J1939 Log");
    let mut table = SimpleTable::new(Box::new(packet_model));
    table.set_font(enums::Font::Screen, 12);
    wind.end();
    wind.resizable(&wind);
    wind.show();

    // repaint the table on a schedule, to demonstrate updating models.
    let timer = Timer::new(); // requires variable, so that it isn't dropped.
    let _redraw_task = timer.schedule_repeating(chrono::Duration::milliseconds(200), move || {
        table.redraw();
    });

    // run the app
    app.run().unwrap();

    Ok(())
}
