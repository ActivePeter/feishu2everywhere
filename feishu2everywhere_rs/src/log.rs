use std::fs::{self, File};

pub const LOG_DIR: &str = "./log";

pub enum LogType {
    ChromeDriver,
}

pub fn new_log_file(logtype: LogType) -> File {
    let file_prefix = match logtype {
        LogType::ChromeDriver => "chrome_driver",
    };
    let now = chrono::Local::now();
    let log_file = format!("{}/{}_{}.log", LOG_DIR, file_prefix, now);

    fs::create_dir_all(LOG_DIR).unwrap();

    let mut file = File::create(log_file).unwrap();

    file
}
