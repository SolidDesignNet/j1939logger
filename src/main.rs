#![windows_subsystem = "windows"]

mod dbc_table;
mod packet_model;
mod packet_repo;

#[derive(RustEmbed)]
#[folder = "assets/"]
struct Asset;

use std::{
    fs::File,
    io::{BufWriter, Write},
    option::Option,
    sync::{Arc, Mutex, RwLock},
    thread,
    time::Duration,
};

use anyhow::Error;
#[cfg(windows)]
use can_adapter::rp1210;
use can_adapter::{
    connection::{self, Connection},
    j1939::J1939,
    packet::J1939Packet,
};
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
    output::Output,
    prelude::{
        GroupExt, InputExt, MenuExt, TableExt, ValuatorExt, WidgetBase, WidgetExt, WindowExt,
    },
    table::Table,
    valuator::HorNiceSlider,
    window::Window,
};
use packet_model::PacketModel;
use packet_repo::PacketRepo;
use rust_embed::RustEmbed;
use simple_table::simple_table::SimpleTable;
use timer::Timer;

fn main() -> Result<(), anyhow::Error> {
    // repaint the table on a schedule, to demonstrate updating models.
    let timer = Arc::new(Timer::new());
    let connection: Arc<Mutex<Option<Box<dyn Connection>>>> = Arc::new(Mutex::new(None));
    let packets = Arc::new(RwLock::new(PacketRepo::default()));

    {
        let connection = connection.clone();
        let packets = packets.clone();
        thread::Builder::new()
            .name("main:packet copy".to_owned())
            .spawn(move || {
                loop {
                    // get iterator from connection if possible
                    if let Some(connection) = (*connection.lock().unwrap()).as_deref_mut() {
                        let mut iter = connection.iter().flatten();
                        let addr = 0xF9;
                        let iter = J1939::receive_tp(connection, addr, false, &mut iter);
                        // make sure to unlock between writes.
                        iter.for_each(|p| packets.write().unwrap().push(&p));
                    }
                    // either no connection or connection closed.
                    thread::sleep(Duration::from_millis(200));
                }
            })?;
    }

    let app = app::App::default().with_scheme(app::Scheme::Gtk);
    app.set_visual(Mode::MultiSample | Mode::Alpha)?;

    let mut wind = Window::default()
        .with_size(400, 600)
        .with_label(&format!("J1939 Log {}", &env!("CARGO_PKG_VERSION")));

    let pack = Pack::default_fill();

    let mut menu = SysMenuBar::default().with_size(100, 35);
    {
        let timer = timer.clone();
        let packets = packets.clone();
        menu.add(
            "&Action/@fileopen Load DBC...\t",
            Shortcut::None,
            menu::MenuFlag::Normal,
            move |_b| {
                load_dbc_window(packets.clone(), &timer).expect("Canceled");
            },
        );
    }
    {
        let list = packets.clone();
        menu.add(
            "&Action/@filesave Save...\t",
            Shortcut::None,
            menu::MenuFlag::Normal,
            move |_| {
                save_log(list.read().unwrap().packets()).expect("Unable to save packet log.");
            },
        );
    }
    {
        let packets = packets.clone();
        menu.add(
            "&Action/@refresh Clear\t",
            Shortcut::None,
            menu::MenuFlag::Normal,
            move |_| {
                packets
                    .write()
                    .expect("Unable to lock model for clear.")
                    .clear();
            },
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
        let list = packets.clone();
        menu.add(
            "&Edit/Copy\t",
            Shortcut::Ctrl | 'c',
            menu::MenuFlag::Normal,
            move |_| {
                let read = list.read().expect("Unable to lock model for copy.");
                let collect: Vec<String> = read.packets().iter().map(|p| format!("{p}")).collect();
                copy(collect.join("\n").as_str());
            },
        );
    }

    add_rp1210_menu(&mut menu, connection.clone())?;

    menu.add(
        "&Action/How to...\t",
        Shortcut::None,
        menu::MenuFlag::Normal,
        move |_b| {
            webbrowser::open("https://github.com/SolidDesignNet/j1939logger/blob/main/README.md")
                .expect("Unable to open web browser.");
        },
    );

    let mut simple_table = SimpleTable::new(table.clone(), PacketModel::new(packets));
    simple_table.set_font(enums::Font::Screen, 18);
    table.end();
    pack.resizable(&table);
    pack.end();

    wind.end();
    wind.resizable(&wind);
    wind.set_icon(Some(PngImage::from_data(
        &Asset::get("can.png")
            .expect("Unable to read icon png.")
            .data,
    )?));
    wind.show();

    simple_table.redraw_on(&timer, chrono::Duration::milliseconds(200));

    // run the app
    app.run()?;

    Ok(())
}

fn load_dbc_window(
    packets: Arc<RwLock<PacketRepo>>,
    timer: &Arc<Timer>,
) -> Result<(), anyhow::Error> {
    let mut fc = FileDialog::new(BrowseMultiFile);
    fc.set_filter("*.dbc");
    fc.show();
    if fc.filenames().is_empty() {
        // canceled
        return Ok(());
    }
    let path = fc.filename();
    let filename = path.to_str().unwrap_or_default();
    let pgns = PgnLibrary::from_dbc_file(path.clone())
        .unwrap_or_else(|_| panic!("Unable to read dbc file {filename}."));
    let model = DbcModel::new(pgns.pgns.values().cloned().collect(), packets.clone());

    let mut wind = Window::default().with_size(600, 300).with_label(filename);
    wind.set_icon(Some(PngImage::from_data(
        &Asset::get("can.png").expect("Unable to load icon.").data,
    )?));

    let pack = Pack::default_fill();

    let mut menu = SysMenuBar::default().with_size(100, 35);

    let mut hbox = Pack::default()
        .with_size(100, 20)
        .with_type(PackType::Horizontal);
    hbox.set_spacing(4);

    let mut slider = HorNiceSlider::default_fill();
    let mut time = Output::default().with_size(80, 20);

    // Why doesn't slider resize?
    hbox.resizable(&slider.as_base_widget());
    hbox.end();

    let mut table = SimpleTable::new(Table::default_fill(), model);
    table.table.end();

    pack.resizable(&table.table);
    pack.end();

    let redraw_period = chrono::Duration::milliseconds(200);
    table.redraw_on(timer, redraw_period);

    let table = Arc::new(Mutex::new(table));
    {
        let table = table.clone();
        slider.set_callback(move |s| {
            let val = s.value();
            let min = s.minimum();
            let percent = (val - min) / (s.maximum() - min);
            if percent > 0.9 {
                time.set_value("Live...");
                table
                    .lock()
                    .unwrap()
                    .model
                    .lock()
                    .unwrap()
                    .set_time(u32::MAX);
            } else {
                time.set_value(&format!("{val:0.2}"));
                table
                    .lock()
                    .unwrap()
                    .model
                    .lock()
                    .unwrap()
                    .set_time(val as u32);
            };
        });

        timer
            .schedule_repeating(redraw_period, move || {
                let (min, max) = {
                    let packet_repo = packets.read().unwrap();
                    (packet_repo.first_time(), packet_repo.last_time())
                };
                slider.set_minimum(min as f64);
                slider.set_maximum(max as f64);
                slider.damage();
            })
            .ignore();
    }
    {
        let table = table.clone();
        menu.add(
            "Action/Map Address...",
            Shortcut::None,
            MenuFlag::Normal,
            move |_| {
                map_address_wizard(table.clone());
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
                let simple_table = &mut table.lock().expect("Unable to lock simple table.");
                simple_table
                    .model
                    .lock()
                    .expect("Unable to lock model.")
                    .toggle_missing();
                simple_table.redraw();
            },
        );
    }
    {
        let mut table = table
            .lock()
            .expect("Unable to lock simple table")
            .table
            .clone();
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
        let table = table.clone();
        menu.add(
            "&Edit/Copy\t",
            Shortcut::Ctrl | 'c',
            menu::MenuFlag::Normal,
            move |_| {
                app::copy(
                    &table
                        .lock()
                        .expect("Unable to lock simple table.")
                        .copy("\t", "\n"),
                );
            },
        );
    }

    wind.end();
    wind.resizable(&wind);
    wind.show();
    Ok(())
}

fn map_address_wizard(table: Arc<Mutex<SimpleTable<DbcModel>>>) {
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

    go.set_callback(move |_| {
        if let (Ok(from), Ok(to)) = (
            u8::from_str_radix(&from.value(), 16),
            u8::from_str_radix(&to.value(), 16),
        ) {
            table
                .lock()
                .expect("Unable to lock simple table")
                .model
                .lock()
                .expect("Unable to lock model.")
                .map_address(from, to);
            wind.hide();
        }
    });
}

fn save_log(list: &[J1939Packet]) -> Result<(), Error> {
    let mut fc = FileDialog::new(fltk::dialog::FileDialogType::BrowseSaveFile);
    fc.show();
    if !fc.filenames().is_empty() {
        let mut out =
            BufWriter::new(File::create(fc.filename()).expect("Failed to create log file."));
        for p in list.iter() {
            out.write_all(p.to_string().as_bytes())
                .expect("Failed to write log file.");
            out.write_all(b"\r\n").expect("Failed to write log file.");
        }
    }
    Ok(())
}
fn add_rp1210_menu(
    menu: &mut SysMenuBar,
    connection: Arc<Mutex<Option<Box<dyn Connection>>>>,
) -> Result<(), Error> {
    #[cfg(windows)]
    menu.add(
        "&Connection/RP1210/Connection String...",
        Shortcut::None,
        menu::MenuFlag::Normal,
        move |_| {
            let mut s = rp1210::CONNECTION_STRING.write().unwrap();
            if let Some(r) = fltk::dialog::input_default("Connection String", &*s) {
                *s = r;
            }
        },
    );

    #[cfg(windows)]
    menu.add(
        "Connection/RP1210/Application Packetization",
        Shortcut::None,
        menu::MenuFlag::Toggle,
        move |_| {
            let mut m = rp1210::APP_PACKETIZATION.write().unwrap();
            *m = !*m;
        },
    );

    add_adapters(menu, &connection)?;

    {
        let connection = connection.clone();
        menu.add(
            "&Connection/@|| Stop",
            Shortcut::None,
            menu::MenuFlag::Normal,
            move |_b| {
                *connection.lock().unwrap() = None;
            },
        );
    }
    Ok(())
}

fn add_adapters(
    menu: &mut SysMenuBar,
    connection: &Arc<Mutex<Option<Box<dyn Connection>>>>,
) -> Result<(), Error> {
    for product in connection::enumerate_connections()? {
        for device in product.devices {
            for factory in device.connections {
                let connection = connection.clone();
                let name = format!(
                    "Connection/{}/{}/{}\t",
                    &product.name,
                    &device.name.replace("/", "\\/"),
                    factory.name()
                );

                menu.add(
                    &name.clone(),
                    Shortcut::None,
                    menu::MenuFlag::Normal,
                    move |_b| {
                        // unload old DLL
                        *connection.lock().unwrap() = None;
                        eprintln!("LOADING: {name}");

                        // load new DLL
                        match factory.create() {
                            Ok(conn) => {
                                *connection.lock().unwrap() = Some(conn);
                            }
                            Err(err) => {
                                message_icon_label("Fail");
                                message_default(&format!("Failed to open adapter: {err}"));
                                *connection.lock().unwrap() = None;
                            }
                        }
                    },
                );
            }
        }
    }
    Ok(())
}
