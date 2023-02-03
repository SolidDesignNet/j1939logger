use std::{
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use anyhow::Error;
use fltk::{
    app,
    enums::{self, Shortcut},
    group::{Pack, PackType},
    menu,
    prelude::{GroupExt, MenuExt, WidgetBase, WidgetExt},
    table::Table,
    window::Window,
};
use rp1210::{multiqueue::MultiQueue, packet::J1939Packet, rp1210::Rp1210, rp1210_parsing};
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
    // let mut rp1210 = ConnectionDescriptor::parse().connect(bus.clone())?;
    // rp1210.run();

    let packet_model = PacketModel::default();
    packet_model.run(bus.clone());

    let app = app::App::default();
    let mut wind = Window::default()
        .with_size(400, 600)
        .with_label("J1939 Log");

    let mut pack = Pack::default_fill();
    pack.set_type(PackType::Vertical);

    let mut menu = menu::SysMenuBar::default().with_size(20, 35);
    let list = packet_model.list.clone();
    menu.add(
        "&Action/Clear\t",
        Shortcut::None,
        menu::MenuFlag::Normal,
        move |_b| {
            list.lock().unwrap().clear();
        },
    );

    add_rp1210_menu(&mut menu, bus.clone())?;

    let table = Table::default();
    let mut simple_table = SimpleTable::new(table.clone(), Box::new(packet_model));
    simple_table.set_font(enums::Font::Screen, 12);

    pack.resizable(&table);
    pack.end();
    wind.end();
    wind.resizable(&wind);
    wind.show();

    // repaint the table on a schedule, to demonstrate updating models.
    let timer = Timer::new(); // requires variable, so that it isn't dropped.
    let _redraw_task = timer.schedule_repeating(chrono::Duration::milliseconds(200), move || {
        simple_table.redraw();
    });

    // run the app
    app.run().unwrap();

    Ok(())
}

fn add_rp1210_menu(menu: &mut menu::SysMenuBar, bus: MultiQueue<J1939Packet>) -> Result<(), Error> {
    for p in rp1210_parsing::list_all_products()? {
        let product_description = p.description.clone();
        for d in p.devices {
            let name = format!("&{}/{} {}\t", &product_description, &d.name, &d.description);
            let bus = bus.clone();
            let dev_name = &d.name;
            let device = d.id;
            menu.add(
                &dev_name,
                Shortcut::None,
                menu::MenuFlag::Normal,
                move |_b| {
                    Rp1210::new(&name, device, "J1939:Baud=Auto", 0xF9, bus.clone())
                        .unwrap()
                        .run();
                },
            );
        }
    }
    //todo!()
    Ok(())
}
