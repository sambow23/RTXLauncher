use std::thread::{self, JoinHandle};
use std::time::Duration;
use std::sync::mpsc::{self, Receiver, Sender};

#[derive(Debug, Clone)]
pub struct JobProgress {
    pub message: String,
    pub percent: u8,
}

pub struct JobHandle {
    pub join: JoinHandle<()>,
    pub rx: Receiver<JobProgress>,
}

pub struct JobRunner;

impl JobRunner {
    pub fn spawn_dummy_job() -> JobHandle {
        let (tx, rx): (Sender<JobProgress>, Receiver<JobProgress>) = mpsc::channel();
        let join = thread::spawn(move || {
            for i in 0..=100u8 {
                let _ = tx.send(JobProgress { message: format!("Working... {i}%"), percent: i });
                thread::sleep(Duration::from_millis(30));
            }
        });
        JobHandle { join, rx }
    }
}


