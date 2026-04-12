use crossterm::event::{self, Event as CEvent, KeyEvent};
use std::sync::mpsc;
use std::time::Duration;

/// Events the TUI main loop processes.
#[derive(Debug)]
pub enum Event {
    /// A key was pressed.
    Key(KeyEvent),
    /// A periodic tick for UI updates.
    Tick,
}

/// Spawns a background thread that reads terminal events and sends them
/// through a channel. This avoids blocking the main async runtime.
pub struct EventHandler {
    rx: mpsc::Receiver<Event>,
    _handle: std::thread::JoinHandle<()>,
}

impl EventHandler {
    pub fn new(tick_rate_ms: u64) -> Self {
        let (tx, rx) = mpsc::channel();
        let tick_duration = Duration::from_millis(tick_rate_ms);

        let handle = std::thread::spawn(move || {
            loop {
                if event::poll(tick_duration).unwrap_or(false) {
                    if let Ok(CEvent::Key(key)) = event::read()
                        && tx.send(Event::Key(key)).is_err()
                    {
                        break;
                    }
                } else if tx.send(Event::Tick).is_err() {
                    break;
                }
            }
        });

        Self {
            rx,
            _handle: handle,
        }
    }

    /// Try to receive the next event (non-blocking).
    #[allow(dead_code)] // Available for async integration
    pub fn try_recv(&self) -> Option<Event> {
        self.rx.try_recv().ok()
    }

    /// Receive the next event (blocking).
    pub fn recv(&self) -> Result<Event, mpsc::RecvError> {
        self.rx.recv()
    }
}
