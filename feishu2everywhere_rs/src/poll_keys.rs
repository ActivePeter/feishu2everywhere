use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};

// ... existing code ...
pub fn start_poll_keys(running_clone: Arc<AtomicBool>) {
    // Replace the keyboard monitoring thread with crossterm implementation
    thread::spawn(move || {
        loop {
            if event::poll(Duration::from_millis(100)).unwrap() {
                if let Ok(Event::Key(KeyEvent {
                    code, modifiers, ..
                })) = event::read()
                {
                    if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
                        println!("Detected Ctrl+C!");
                        running_clone.store(false, Ordering::SeqCst);
                        break;
                    }
                }
            }
            // Still sleep a bit to avoid high CPU usage
            thread::sleep(Duration::from_millis(10));
        }
    });
}
