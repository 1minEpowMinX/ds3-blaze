//! Process-wide broadcast of log lines and state-change notices.
//!
//! The channel is one per process, like the log file: [`crate::log!`] is called
//! from every module and every task, and a receiver sees the lines of all of them.

use std::fmt;
use std::sync::LazyLock;
use tokio::sync::broadcast;

/// Number of events a receiver may fall behind by before it misses the oldest.
pub const CAPACITY: usize = 1024;

static SENDER: LazyLock<broadcast::Sender<Event>> =
    LazyLock::new(|| broadcast::channel(CAPACITY).0);

/// Something a window or a console shows.
#[derive(Clone, Debug)]
pub enum Event {
    /// A line written with [`crate::log!`].
    Log(LogLine),
    /// The server's state changed; [`crate::server::ServerHandle::snapshot`]
    /// returns the new one.
    Changed,
}

/// One log line: local time, tag and text.
#[derive(Clone, Debug)]
pub struct LogLine {
    pub time: chrono::DateTime<chrono::Local>,
    pub tag: String,
    pub text: String,
}

impl fmt::Display for LogLine {
    /// Writes the line as the console and the log file show it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} [{}] {}",
            self.time.format("%H:%M:%S%.3f"),
            self.tag,
            self.text
        )
    }
}

/// Returns a receiver of every event sent from now on.
pub fn subscribe() -> broadcast::Receiver<Event> {
    SENDER.subscribe()
}

/// Sends `event` to every receiver; without receivers the event is dropped.
pub fn send(event: Event) {
    let _ = SENDER.send(event);
}

/// Sends [`Event::Changed`].
pub fn changed() {
    send(Event::Changed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_line_reads_like_the_console() {
        let time = chrono::Local::now();
        let line = LogLine {
            time,
            tag: "blaze#1".into(),
            text: "connected".into(),
        };
        assert_eq!(
            line.to_string(),
            format!("{} [blaze#1] connected", time.format("%H:%M:%S%.3f"))
        );
    }

    #[test]
    fn log_macro_reaches_a_subscriber() {
        let mut events = subscribe();
        crate::log!("event-test", "line {}", 1);
        // Other tests in this process log too; only this test uses the tag.
        loop {
            match events.try_recv() {
                Ok(Event::Log(line)) if line.tag == "event-test" => {
                    assert_eq!(line.text, "line 1");
                    return;
                }
                Ok(_) => {}
                Err(err) => panic!("no log event: {err:?}"),
            }
        }
    }
}
