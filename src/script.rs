use std::time::SystemTime;

use anyhow::{Ok, Result};
use chrono::Duration;
use regex::Regex;
use rp1210::packet::J1939Packet;

trait Command {
    fn execute(
        &mut self,
        bus: &mut dyn Bus<J1939Packet>,
        packet: &Option<J1939Packet>,
    ) -> Result<()>;
}
struct Script {
    commands: Vec<Box<dyn Command>>,
}
trait Bus<T> {
    fn iter_for(&mut self, duration: Duration) -> Box<dyn Iterator<Item = T>>;
    fn send(&mut self, packet: &T) -> anyhow::Result<T>;
}

impl Script {
    pub fn run(&mut self, bus: &mut dyn Bus<J1939Packet>) {
        bus.iter_for(Duration::max_value()).for_each(|p| {
            self.commands.iter_mut().for_each(|c| {
                let _ = c.as_mut().execute(bus, &Some(p.clone()));
            })
        });
    }
}

struct ScheduledSend {
    packet: J1939Packet,
    previous: SystemTime,
}

impl Command for ScheduledSend {
    fn execute(&mut self, bus: &mut dyn Bus<J1939Packet>, _: &Option<J1939Packet>) -> Result<()> {
        let now = SystemTime::now();
        if now.lt(&self.previous) {
            bus.send(&self.packet)?;
            self.previous = now;
        }
        Ok(())
    }
}
struct Response {
    pattern: Regex,
    packet: J1939Packet,
}
impl Command for Response {
    fn execute(
        &mut self,
        bus: &mut dyn Bus<J1939Packet>,
        packet: &Option<J1939Packet>,
    ) -> Result<()> {
        if packet
            .clone()
            .map_or(false, |p| self.pattern.is_match(&p.to_string()))
        {
            bus.send(&self.packet)?;
        }
        Ok(())
    }
}
