#![windows_subsystem = "windows"]

mod dbc_table;

#[derive(RustEmbed)]
#[folder = "assets/"]
struct Asset;

use std::{
    cell::RefCell,
    collections::HashMap,
    fs::File,
    io::{BufWriter, Write},
    option::Option,
    sync::{Arc, Mutex, RwLock},
    thread,
    time::Duration,
};

use anyhow::Error;
use canparse::pgn::PgnLibrary;
use dbc_table::DbcModel;
use fltk::{
    app,
    dialog::{message_default, message_icon_label, FileDialog, FileDialogType::BrowseMultiFile},
    enums::{self, Shortcut},
    group::Pack,
    image::PngImage,
    input::Input,
    menu::{self, SysMenuBar},
    prelude::{GroupExt, InputExt, MenuExt, TableExt, WidgetBase, WidgetExt, WindowExt},
    table::Table,
    window::Window,
};
use rp1210::{multiqueue::MultiQueue, packet::J1939Packet, rp1210::Rp1210, rp1210_parsing};
use rust_embed::RustEmbed;
use simple_table::simple_table::{SimpleModel, SimpleTable};
use timer::Timer;

/// simple table model to represent log
#[derive(Default)]
struct PacketModel {
    pub list: Arc<RwLock<Vec<Arc<J1939Packet>>>>,
    index: Arc<RwLock<HashMap<u32, Arc<J1939Packet>>>>,
}

impl PacketModel {
    /// copy packets from bus to table
    pub fn run(&self, bus: MultiQueue<J1939Packet>) -> thread::JoinHandle<()> {
        let list = self.list.clone();
        let index = self.index.clone();
        thread::spawn(move || {
            bus.iter_for(Duration::from_secs(60 * 60 * 24 * 7))
                .for_each(|p| {
                    let p = Arc::new(p);
                    list.write().unwrap().push(p.clone());
                    index.write().unwrap().insert(p.id(), p);
                })
        })
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
}

fn main() -> Result<(), anyhow::Error> {
    // repaint the table on a schedule, to demonstrate updating models.
    let timer = Arc::new(Timer::new());

    let bus: MultiQueue<J1939Packet> = MultiQueue::new();

    let packet_model = PacketModel::default();
    packet_model.run(bus.clone());

    let app = app::App::default();
    let mut wind = Window::default()
        .with_size(400, 600)
        .with_label(&format!("J1939 Log {}", &env!("CARGO_PKG_VERSION")));

    let pack = Pack::default_fill();

    // // this needs to be right of the menu (you don't have to go home, But you can't stay here)
    let mut connection_string = Input::default()
        .with_label("Connection String")
        .with_size(100, 32);
    connection_string.set_value("J1939:Channel=1;Baud=500");

    create_menu(
        SysMenuBar::default().with_size(100, 35),
        &packet_model,
        connection_string,
        bus,
        timer.clone(),
    )?;

    let list = packet_model.list.clone();

    let mut table = Table::default_fill();
    let mut simple_table = SimpleTable::new(table.clone(), Box::new(packet_model));
    simple_table.set_font(enums::Font::Screen, 12);
    table.end();

    pack.end();
    wind.end();
    wind.resizable(&wind);
    wind.set_icon(Some(PngImage::from_data(
        &Asset::get("cancan.png").unwrap().data,
    )?));
    wind.show();

   // simple_table.redraw_on(&timer, chrono::Duration::milliseconds(200));

    // run the app
    app.run().unwrap();

    Ok(())
}

fn create_menu(
    mut menu: SysMenuBar,
    packet_model: &PacketModel,
    connection_string: Input,
    bus: MultiQueue<J1939Packet>,
    timer: Arc<Timer>,
) -> Result<(), Error> {
    let index = packet_model.index.clone();
    menu.add(
        "&Action/Load DBC...\t",
        Shortcut::None,
        menu::MenuFlag::Normal,
        move |_b| {
            // request file from user
            let mut fc = FileDialog::new(BrowseMultiFile);
            fc.show();
            if fc.filenames().is_empty() {
                return;
            }
            let filename = fc.filename();
            let mut wind = Window::default()
                .with_size(300, 300)
                .with_label(filename.to_str().unwrap());

            {
                let index = index.clone();
                let model = DbcModel::new(
                    PgnLibrary::from_dbc_file(filename).unwrap(),
                    Box::new(move |id| -> Option<Arc<J1939Packet>> {
                        index.read().unwrap().get(&id).map(|a| a.clone())
                    }),
                );
                // allocation has a side effect in FLTK
                let mut simple_table = SimpleTable::new(Table::default_fill(), Box::new(model));
                eprintln!("configure redraw_on");
                simple_table.redraw_on(&timer, chrono::Duration::milliseconds(200));
            };

            wind.end();
            wind.resizable(&wind);
            wind.show();
        },
    );
    {
        let list = packet_model.list.clone();
        menu.add(
            "&Action/Save...\t",
            Shortcut::None,
            menu::MenuFlag::Normal,
            move |_| -> () {
                save_log(list.clone());
            },
        );
    }
    {
        let list = packet_model.list.clone();
        let index = packet_model.index.clone();
        menu.add(
            "&Action/Clear\t",
            Shortcut::None,
            menu::MenuFlag::Normal,
            move |_| {
                list.write().unwrap().clear();
                index.write().unwrap().clear();
            },
        );
    }
    add_rp1210_menu(
        Box::new(move || connection_string.value()),
        &mut menu,
        bus.clone(),
    )?;
    Ok(())
}

fn save_log(list: Arc<RwLock<Vec<Arc<J1939Packet>>>>) -> () {
    let mut fc = FileDialog::new(fltk::dialog::FileDialogType::BrowseSaveFile);
    fc.show();
    if fc.filenames().is_empty() {
        return;
    }
    let mut out = BufWriter::new(File::create(fc.filename()).expect("Failed to create log file."));
    for p in list.read().unwrap().iter() {
        out.write_all(p.to_string().as_bytes())
            .expect("Failed to write log file.");
        out.write_all(b"\r\n").expect("Failed to write log file.");
    }
}

fn add_rp1210_menu(
    connection_string_fn: Box<dyn Fn() -> String>,
    menu: &mut SysMenuBar,
    bus: MultiQueue<J1939Packet>,
) -> Result<(), Error> {
    let adapter = Arc::new(RefCell::new(Option::None));

    let connection_string_fn = Arc::new(connection_string_fn);
    for product in rp1210_parsing::list_all_products()? {
        for device in product.devices {
            let name = format!("&RP1210/{}/{}\t", &product.description, &device.description);
            let id = product.id.clone();
            let bus = bus.clone();
            let device_id = device.id;
            let adapter = adapter.clone();
            let cs_fn = connection_string_fn.clone();
            menu.add(&name, Shortcut::None, menu::MenuFlag::Normal, move |_b| {
                // unload old DLL
                adapter.replace(None);
                eprintln!("LOADING: {} {}", id, cs_fn());
                // load new DLL
                match Rp1210::new(id.as_str(), device_id, cs_fn().as_str(), 0xF9, bus.clone()) {
                    Ok(mut rp1210) => {
                        rp1210.run();
                        adapter.replace(Some(rp1210));
                    }
                    Err(err) => {
                        message_icon_label("Fail");
                        message_default(&format!("Failed to open adapter: {}", err));
                    }
                }
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
