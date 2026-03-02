// TODO: 实现错误处理

use std::path::PathBuf;

pub(crate) trait Logger {
    fn log(&self, message: &str);
}

pub(crate) struct NoLogger;

impl Logger for NoLogger {
    fn log(&self, _message: &str) {
        // No operation
    }
}

pub(crate) struct FileLogger {
    pub(crate) path: PathBuf,
}

impl Logger for FileLogger {
    fn log(&self, message: &str) {
        use std::fs::OpenOptions;
        use std::io::Write;

        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&self.path)
            .unwrap();

        writeln!(&mut file, "{}", message).unwrap();
    }
}
