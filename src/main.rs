use std::{
    cell::RefCell,
    fs::File,
    io::{BufWriter, Write},
    option::Option,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use anyhow::Error;
use fltk::{
    app,
    dialog::FileDialog,
    enums::{self, Shortcut},
    group::Pack,
    input::Input,
    menu::{self, SysMenuBar},
    prelude::{GroupExt, InputExt, MenuExt, TableExt, WidgetBase, WidgetExt},
    table::Table,
    window::Window,
};
use rp1210::{multiqueue::MultiQueue, packet::J1939Packet, rp1210::Rp1210, rp1210_parsing};
use simple_table::simple_table::{SimpleModel, SimpleTable};
use timer::Timer;

/// simple table model to represent log
#[derive(Default)]
struct PacketModel {
    pub list: Arc<Mutex<Vec<J1939Packet>>>,
}

impl PacketModel {
    /// copy packets from bus to table
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

    let packet_model = PacketModel::default();
    packet_model.run(bus.clone());

    let app = app::App::default();
    let mut wind = Window::default()
        .with_size(400, 600)
        .with_label("J1939 Log");

    let pack = Pack::default_fill();

    // this needs to be right of the menu (you don't have to go home, But you can't stay here)
    let mut connection_string = Input::default()
        .with_label("Connection String")
        .with_size(100, 32);
    connection_string.set_value("J1939:Baud=auto");

    let mut menu = SysMenuBar::default().with_size(100, 35);
    {
        let list = packet_model.list.clone();
        menu.add(
            "&Action/Save...\t",
            Shortcut::None,
            menu::MenuFlag::Normal,
            move |_b| {
                let mut fc = FileDialog::new(fltk::dialog::FileDialogType::BrowseSaveFile);
                fc.show();
                let mut out = BufWriter::new(
                    File::create(fc.filename()).expect("Failed to create log file."),
                );
                for p in list.lock().unwrap().iter() {
                    out.write_all(p.to_string().as_bytes())
                        .expect("Failed to write log file.");
                    out.write_all(b"\r\n").expect("Failed to write log file.");
                }
            },
        );
    }
    {
        let list = packet_model.list.clone();
        menu.add(
            "&Action/Clear\t",
            Shortcut::None,
            menu::MenuFlag::Normal,
            move |_b| {
                list.lock().unwrap().clear();
            },
        );
    }
    add_rp1210_menu(
        Box::new(move || connection_string.value()),
        &mut menu,
        bus.clone(),
    )?;

    let list = packet_model.list.clone();
    let mut table = Table::default();
    let mut simple_table = SimpleTable::new(table.clone(), Box::new(packet_model));
    simple_table.set_font(enums::Font::Screen, 12);

    pack.resizable(&table);
    pack.end();
    wind.end();
    wind.resizable(&wind);
    wind.show();

    // repaint the table on a schedule, to demonstrate updating models.
    let timer = Timer::new();
    let _redraw_task = timer.schedule_repeating(chrono::Duration::milliseconds(200), move || {
        let row = list.lock().unwrap().len() as i32;
        if table.row_position() > (0.9 * (row as f64)) as i32 {
            table.set_row_position(row);
        }
        simple_table.redraw();
    });

    // run the app
    app.run().unwrap();

    Ok(())
}

fn add_rp1210_menu(
    connection_string_fn: Box<dyn Fn() -> String>,
    menu: &mut SysMenuBar,
    bus: MultiQueue<J1939Packet>,
) -> Result<(), Error> {
    let adapter = Arc::new(RefCell::new(Option::None));

    let connection_string_fn = Arc::new(connection_string_fn);
    for p in rp1210_parsing::list_all_products()? {
        let product_description = p.id.clone();
        for device in p.devices {
            let id = p.id.clone();
            let name = format!(
                "&RP1210/{}/{}: {}\t",
                &product_description, &device.name, &device.description
            );
            let bus = bus.clone();
            let device_id = device.id;
            let adapter = adapter.clone();
            let cs_fn = connection_string_fn.clone();
            menu.add(&name, Shortcut::None, menu::MenuFlag::Normal, move |_b| {
                // unload old DLL
                adapter.replace(None);
                eprintln!("LOADING: {} {}", id, cs_fn());
                // load new DLL
                let mut rp1210 =
                    Rp1210::new(id.as_str(), device_id, cs_fn().as_str(), 0xF9, bus.clone())
                        .unwrap();
                rp1210.run();
                adapter.replace(Some(rp1210));
            });
        }
    }
    menu.add(
        "&RP1210/Stop",
        Shortcut::None,
        menu::MenuFlag::Normal,
        move |_b| {
            adapter.replace(None);
        },
    );
    Ok(())
}
