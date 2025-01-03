use can_adapter::bus::Bus;
use can_adapter::bus::PushBus;
use can_adapter::connection::Connection;
use can_adapter::packet::J1939Packet;
use socketcan::Socket;

use std::option::Option;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;
use std::time::SystemTime;
use socketcan::CanSocket;
use std::sync::Mutex;
use std::sync::Arc;

/// ```sh
///   ip link set can0 up
///   ip link set can0 type can bitrate 500000
/// ```
#[derive(Clone)]
pub struct SocketCanConnection {
    socket: Arc<Mutex<CanSocket>>,
    bus: Box<PushBus<J1939Packet>>,
    running: Arc<AtomicBool>,
    start: SystemTime,
}

impl SocketCanConnection {
    pub fn new(str: &str) -> Result<SocketCanConnection, anyhow::Error> {
        let socket_can_connection = SocketCanConnection {
            socket: Arc::new(Mutex::new(CanSocket::open(str)?)),
            bus: Box::new(PushBus::new()),
            running: Arc::new(AtomicBool::new(false)),
            start: SystemTime::now(),
        };
        let mut scc = socket_can_connection.clone();
        thread::spawn(move || scc.run());
        Ok(socket_can_connection)
    }
    fn run(&mut self) {
        self.running.store(true, Ordering::Relaxed);
        while self.running.load(Ordering::Relaxed) {
            let read_raw_frame = self.socket.lock().unwrap().read_raw_frame();
            let p = if read_raw_frame.is_ok() {
                let frame = read_raw_frame.unwrap();
                Some(J1939Packet::new_socketcan(
                    self.now(),
                    false,
                    frame.can_id,
                    &frame.data,
                ))
            } else {
                std::thread::sleep(Duration::from_millis(100));
                None
            };
            self.bus.push(p);
        }
    }
    fn now(&self) -> u32 {
        SystemTime::now()
            .duration_since(self.start)
            .expect("Time went backwards")
            .as_millis() as u32
    }
}

impl Connection for SocketCanConnection {
    fn send(&mut self, packet: &J1939Packet) -> Result<J1939Packet, anyhow::Error> {
        todo!()
    }

    fn iter(&self) -> Box<dyn Iterator<Item = Option<J1939Packet>> + Send + Sync> {
        self.bus.iter()
    }
}
impl Drop for SocketCanConnection {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        self.bus.close();
        //let _ = self.thread.take().unwrap().join();
    }
}
