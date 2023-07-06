#![windows_subsystem = "windows"]

mod dbc_table;

#[derive(RustEmbed)]
#[folder = "assets/"]
struct Asset;

use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    fs::File,
    io::{BufWriter, Write},
    option::Option,
    sync::{Arc, RwLock},
    thread,
    time::{Duration, Instant},
};

use anyhow::Error;
use canparse::pgn::PgnLibrary;
use dbc_table::DbcModel;
use fltk::{
    app,
    button::{Button, CheckButton},
    dialog::{message_default, message_icon_label, FileDialog, FileDialogType::BrowseMultiFile},
    enums::{self, Mode, Shortcut},
    frame::Frame,
    group::{Pack, PackType},
    image::PngImage,
    input::Input,
    menu::{self, SysMenuBar},
    prelude::{GroupExt, InputExt, MenuExt, WidgetBase, WidgetExt, WindowExt},
    table::Table,
    window::Window,
};
use rp1210::{multiqueue::MultiQueue, packet::J1939Packet, rp1210::Rp1210, rp1210_parsing};
use rust_embed::RustEmbed;
use simple_table::simple_table::{Order, SimpleModel, SimpleTable};
use timer::Timer;

/// simple table model to represent log
#[derive(Clone, Default)]
struct PacketModel {
    pub list: Arc<RwLock<Vec<J1939Packet>>>,
    table: Arc<RwLock<HashMap<u32, VecDeque<J1939Packet>>>>,
}

impl PacketModel {
    /// copy packets from bus to table
    pub fn run(&self, bus: MultiQueue<J1939Packet>) -> thread::JoinHandle<()> {
        let list = self.list.clone();
        let table = self.table.clone();
        let mut last_trim = Instant::now();
        thread::spawn(move || {
            bus.iter_for(Duration::from_secs(60 * 60 * 24 * 7))
                .for_each(|p| {
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

    fn sort(&mut self, _col: usize, _order: Order) {
        // sorting not supported
    }
}

fn main() -> Result<(), anyhow::Error> {
    // repaint the table on a schedule, to demonstrate updating models.
    let timer = Arc::new(Timer::new());

    let bus: MultiQueue<J1939Packet> = MultiQueue::new();

    let packet_model = PacketModel::default();
    packet_model.run(bus.clone());

    let app = app::App::default().with_scheme(app::Scheme::Oxy);
    app.set_visual(Mode::MultiSample | Mode::Alpha)?;

    let mut wind = Window::default()
        .with_size(400, 600)
        .with_label(&format!("J1939 Log {}", &env!("CARGO_PKG_VERSION")));

    let pack = Pack::default_fill();

    // // this needs to be right of the menu (you don't have to go home, But you can't stay here)
    let connection_string_fn = {
        let mut connection_string = Input::default()
            .with_label("Connection String")
            .with_size(100, 32);
        connection_string.set_value("J1939:Baud=500");
        move || connection_string.value()
    };

    let channel_fn = {
        let channel_pack = Pack::default()
            .with_size(100, 32)
            .with_type(PackType::Horizontal);
        channel_pack.begin();
        Frame::default().with_label("Channels").with_size(80, 32);
        let check_buttons = [1, 2, 3].map(|c| {
            CheckButton::default()
                .with_label(c.to_string().as_str())
                .with_size(32, 32)
        });
        check_buttons[0].set_checked(true);
        channel_pack.end();
        move || {
            check_buttons
                .iter()
                .filter(|c| c.is_checked())
                .map(|b| b.label().parse::<u8>().unwrap())
                .collect()
        }
    };

    create_menu(
        SysMenuBar::default().with_size(100, 35),
        &packet_model,
        connection_string_fn,
        channel_fn,
        bus,
        timer.clone(),
    )?;

    let table = Table::default_fill();
    let mut simple_table = SimpleTable::new(table.clone(), Box::new(packet_model));
    simple_table.set_font(enums::Font::Screen, 18);
    table.end();
    pack.resizable(&table);
    pack.end();

    wind.end();
    wind.resizable(&wind);
    wind.set_icon(Some(PngImage::from_data(
        &Asset::get("cancan.png").unwrap().data,
    )?));
    wind.show();

    simple_table.redraw_on(&timer, chrono::Duration::milliseconds(200));

    // run the app
    app.run().unwrap();

    Ok(())
}

fn create_menu(
    mut menu: SysMenuBar,
    packet_model: &PacketModel,
    connection_string_fn: impl Fn() -> String + 'static,
    channels_fn: impl Fn() -> Vec<u8> + 'static,
    bus: MultiQueue<J1939Packet>,
    timer: Arc<Timer>,
) -> Result<(), Error> {
    let table = packet_model.table.clone();
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
            let lib = PgnLibrary::from_dbc_file(filename.clone()).unwrap();
            let mut wind = Window::default()
                .with_size(600, 300)
                .with_label(filename.to_str().unwrap());

            let pack = Pack::default_fill();

            let hpack = Pack::default_fill()
                .with_size(600, 20)
                .with_type(PackType::Horizontal);
            let mut hide_mising_button = CheckButton::default()
                .with_label("hide missing")
                .with_size(150, 20);
            let from_addr = Input::default()
                //.with_type(InputType::Int)
                .with_label("From")
                .with_size(100, 20);
            let to_addr = Input::default()
                //.with_type(InputType::Int)
                .with_label("To")
                .with_size(100, 20);
            let mut map_addr = Button::default().with_label("Map Addr").with_size(100, 20);
            hpack.resizable(&from_addr);
            hpack.resizable(&to_addr);
            hpack.end();

            // allocation has a side effect in FLTK
            let model = DbcModel::new(lib, table.clone());
            let mut table = SimpleTable::new(Table::default_fill(), Box::new(model));
            table.table.end();

            pack.resizable(&table.table);
            pack.end();

            table.redraw_on(&timer, chrono::Duration::milliseconds(200));

            let model = table.model.clone();
            let model2 = table.model.clone();
            hide_mising_button.set_callback(move |_| {
                println!("hiding rows {}", model.lock().unwrap().row_count());
                model.lock().unwrap().remove_missing();
                table.redraw();
                println!("   hid rows {}", model.lock().unwrap().row_count());
            });
            map_addr.set_callback(move |_| {
                println!("map");
                let from = u8::from_str_radix(&from_addr.value(),16);
                let to = u8::from_str_radix(&to_addr.value(),16);
                println!("maping from {:?} to {:?}", from, to);
                if from.is_ok() && to.is_ok() {
                    model2
                        .lock()
                        .unwrap()
                        .map_address(from.unwrap(), to.unwrap());
                }
            });
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
            move |_| -> () { save_log(list.clone()) },
        );
    }
    {
        let list = packet_model.list.clone();
        menu.add(
            "&Action/Clear\t",
            Shortcut::None,
            menu::MenuFlag::Normal,
            move |_| list.write().unwrap().clear(),
        );
    }
    add_rp1210_menu(connection_string_fn, channels_fn, &mut menu, bus.clone())?;
    Ok(())
}

fn save_log(list: Arc<RwLock<Vec<J1939Packet>>>) -> () {
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
    connection_string_fn: impl Fn() -> String + 'static,
    channels_fn: impl Fn() -> Vec<u8> + 'static,
    menu: &mut SysMenuBar,
    bus: MultiQueue<J1939Packet>,
) -> Result<(), Error> {
    let adapter = Arc::new(RefCell::new(Option::None));

    let connection_string_fn = Arc::new(connection_string_fn);
    let channels_fn = Arc::new(channels_fn);

    for product in rp1210_parsing::list_all_products()? {
        for device in product.devices {
            let name = format!("&RP1210/{}/{}\t", &product.description, &device.description);
            let id = product.id.clone();
            let bus = bus.clone();
            let device_id = device.id;
            let adapter = adapter.clone();

            let connection_string_fn = connection_string_fn.clone();
            let channels_fn = channels_fn.clone();

            menu.add(&name, Shortcut::None, menu::MenuFlag::Normal, move |_b| {
                // unload old DLL
                adapter.replace(None);
                //eprintln!("LOADING: {} {}", id, cs_fn.clone()());

                // load new DLL
                match Rp1210::new(
                    id.as_str(),
                    device_id,
                    connection_string_fn().as_str(),
                    0xF9,
                    bus.clone(),
                ) {
                    Ok(mut rp1210) => {
                        let channels_fn = channels_fn();
                        if channels_fn.is_empty() {
                            rp1210
                                .run(None)
                                .expect("Failed to open adapter with default channel");
                        } else {
                            for channel in channels_fn {
                                rp1210
                                    .run(Some(channel))
                                    .expect(format!("Failed to open channel {}", channel).as_str());
                            }
                        }
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
