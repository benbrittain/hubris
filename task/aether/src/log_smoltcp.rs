pub struct SysLogger;

impl log::Log for SysLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &log::Record) {
        userlib::sys_log!("{} - {}", record.level(), record.args());
    }
    fn flush(&self) {}
}
