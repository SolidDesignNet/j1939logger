#![windows_subsystem = "windows"]

mod dbc_table;
mod packet_model;

#[derive(RustEmbed)]
#[folder = "assets/"]
struct Asset;

use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    fs::File,
    io::{BufWriter, Write},
    option::Option,
    sync::{Arc, Mutex, RwLock},
};

use anyhow::Error;
use canparse::pgn::PgnLibrary;
use dbc_table::DbcModel;
use fltk::{
    app::{self, copy},
    button::Button,
    dialog::{message_default, message_icon_label, FileDialog, FileDialogType::BrowseMultiFile},
    enums::{self, Mode, Shortcut},
    frame::Frame,
    group::{Flex, Pack, PackType},
    image::PngImage,
    input::Input,
    menu::{self, MenuFlag, SysMenuBar},
    prelude::{GroupExt, InputExt, MenuExt, TableExt, WidgetBase, WidgetExt, WindowExt},
    table::Table,
    window::Window,
};
use packet_model::PacketModel;
use rp1210::{multiqueue::MultiQueue, packet::J1939Packet, rp1210::Rp1210, rp1210_parsing};
use rust_embed::RustEmbed;
use simple_table::simple_table::SimpleTable;
use timer::Timer;

fn main() -> Result<(), anyhow::Error> {
    // repaint the table on a schedule, to demonstrate updating models.
    let timer = Arc::new(Timer::new());

    let bus: MultiQueue<J1939Packet> = MultiQueue::new();

    let packet_model = PacketModel::default();
    packet_model.run(bus.clone());

    let app = app::App::default().with_scheme(app::Scheme::Gtk);
    app.set_visual(Mode::MultiSample | Mode::Alpha)?;

    let mut wind = Window::default()
        .with_size(400, 600)
        .with_label(&format!("J1939 Log {}", &env!("CARGO_PKG_VERSION")));

    let pack = Pack::default_fill();

    let mut menu = SysMenuBar::default().with_size(100, 35);
    let pm2 = &packet_model;
    let timer2 = timer.clone();
    let table = pm2.table.clone();
    menu.add(
        "&Action/@fileopen Load DBC...\t",
        Shortcut::None,
        menu::MenuFlag::Normal,
        move |_b| load_dbc_window(&table, &timer2),
    );
    {
        let list = pm2.list.clone();
        menu.add(
            "&Action/@filesave Save...\t",
            Shortcut::None,
            menu::MenuFlag::Normal,
            move |_| -> () { save_log(&list) },
        );
    }
    {
        let list = pm2.list.clone();
        menu.add(
            "&Action/@refresh Clear\t",
            Shortcut::None,
            menu::MenuFlag::Normal,
            move |_| list.write().unwrap().clear(),
        );
    }

    let table = Table::default_fill();
    {
        let mut table = table.clone();
        menu.add(
            "&Edit/Select All\t",
            Shortcut::Ctrl | 'a',
            menu::MenuFlag::Normal,
            move |_| {
                table.set_selection(0, 0, table.rows(), table.cols());
            },
        );
    }
    {
        let list = packet_model.list.clone();
        menu.add(
            "&Edit/Copy\t",
            Shortcut::Ctrl | 'c',
            menu::MenuFlag::Normal,
            move |_| {
                let read = &list.read().unwrap();
                let collect: Vec<String> = read.iter().map(|p| format!("{}", p)).collect();
                copy(collect.join("\n").as_str());
            },
        );
    }

    add_rp1210_menu(&mut menu, bus.clone())?;

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

fn load_dbc_window(table: &Arc<RwLock<HashMap<u32, VecDeque<J1939Packet>>>>, timer: &Arc<Timer>) {
    let mut fc = FileDialog::new(BrowseMultiFile);
    fc.show();
    if fc.filenames().is_empty() {
        return;
    }
    let filename = fc.filename();
    let model = DbcModel::new(
        PgnLibrary::from_dbc_file(filename.clone()).unwrap(),
        table.clone(),
    );

    let mut wind = Window::default()
        .with_size(600, 300)
        .with_label(filename.to_str().unwrap());

    let pack = Pack::default_fill();

    let mut menu = SysMenuBar::default().with_size(100, 35);
    let mut table = SimpleTable::new(Table::default_fill(), Box::new(model));
    table.table.end();

    pack.resizable(&table.table);
    pack.end();
    table.redraw_on(&timer, chrono::Duration::milliseconds(200));

    let table = Arc::new(Mutex::new(table));
    {
        let table = table.clone();
        menu.add(
            "Action/Map Address...",
            Shortcut::None,
            MenuFlag::Normal,
            move |_| {
                map_address_wizard(&table);
            },
        );
    }
    {
        let table = table.clone();
        menu.add(
            "Action/Hide Inactive",
            Shortcut::None,
            MenuFlag::Toggle,
            move |_| {
                let simple_table = &mut table.lock().unwrap();
                simple_table.model.lock().unwrap().toggle_missing();
                simple_table.redraw();
            },
        );
    }
    wind.end();
    wind.resizable(&wind);
    wind.show();
}

fn map_address_wizard(table: &Arc<Mutex<SimpleTable<DbcModel>>>) {
    let mut wind = Window::default()
        .with_size(100, 180)
        .with_label("Map Address");

    let pack = Flex::default_fill()
        .with_type(PackType::Vertical)
        .size_of(&wind);

    Frame::default().with_label("From (hex)");
    let mut from = Input::default().with_size(35, 35);
    from.set_value("FE");
    Frame::default().with_label("To (hex)");
    let mut to = Input::default_fill().with_size(35, 35).with_label("To");
    to.set_value("00");
    let mut go = Button::default_fill()
        .with_size(35, 35)
        .with_label("Update");

    pack.end();

    wind.end();
    wind.resizable(&pack);
    wind.show();

    let table = table.clone();
    go.set_callback(move |_| {
        let from = u8::from_str_radix(&from.value(), 16);
        let to = u8::from_str_radix(&to.value(), 16);
        if from.is_ok() && to.is_ok() {
            table
                .lock()
                .unwrap()
                .model
                .lock()
                .unwrap()
                .map_address(from.unwrap(), to.unwrap());
            wind.hide();
        }
    });
}

fn save_log(list: &Arc<RwLock<Vec<J1939Packet>>>) -> () {
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

fn add_rp1210_menu(menu: &mut SysMenuBar, bus: MultiQueue<J1939Packet>) -> Result<(), Error> {
    let connection_string = Arc::new(Mutex::new("J1939:Baud=500".to_string()));

    let connection_string2 = connection_string.clone();
    menu.add(
        "&RP1210/Connection String...",
        Shortcut::None,
        menu::MenuFlag::Normal,
        move |_| {
            let s = connection_string2.lock();
            if let Ok(mut str) = s {
                if let Some(r) = fltk::dialog::input_default("Connection String", &*str) {
                    *str = r;
                }
            }
        },
    );

    let channels = Arc::new(Mutex::new(vec![1]));
    let c = channels.clone();
    menu.add(
        "RP1210/Channel 1",
        Shortcut::None,
        menu::MenuFlag::Radio,
        move |_| channel_select(&c, 1),
    );
    let c = channels.clone();
    menu.add(
        "RP1210/Channel 2",
        Shortcut::None,
        menu::MenuFlag::Radio,
        move |_| channel_select(&c, 2),
    );
    let c = channels.clone();
    menu.add(
        "_RP1210/Channel 3",
        Shortcut::None,
        menu::MenuFlag::Radio,
        move |_| channel_select(&c, 3),
    );

    let adapter = Arc::new(RefCell::new(Option::None));

    for product in rp1210_parsing::list_all_products()? {
        for device in product.devices {
            let name = format!(
                "RP1210/{}/@> {}\t",
                &product.description, &device.description
            );
            let id = product.id.clone();
            let bus = bus.clone();
            let device_id = device.id;
            let adapter = adapter.clone();

            let cs = connection_string.clone();
            let channels = channels.clone();
            menu.add(&name, Shortcut::None, menu::MenuFlag::Normal, move |_b| {
                // unload old DLL
                adapter.replace(None);
                eprintln!(
                    "LOADING: {} {} channels: {:?}",
                    id,
                    cs.lock().unwrap(),
                    channels.lock().unwrap()
                );

                // load new DLL
                let lock = cs.lock();
                let connection_string = &*lock.unwrap();
                match Rp1210::new(id.as_str(), device_id, connection_string, 0xF9, bus.clone()) {
                    Ok(mut rp1210) => {
                        let channels = &channels.lock().unwrap();
                        if channels.is_empty() {
                            rp1210
                                .run(None)
                                .expect("Failed to open adapter with default channel");
                        } else {
                            for channel in channels.iter() {
                                rp1210
                                    .run(Some(*channel))
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
        "&RP1210/@|| Stop",
        Shortcut::None,
        menu::MenuFlag::Normal,
        move |_b| {
            adapter.replace(None);
        },
    );
    Ok(())
}

fn channel_select(c: &Arc<Mutex<Vec<u8>>>, channel: u8) {
    let mut cb = c.lock().unwrap();
    cb.clear();
    cb.push(channel);
}
