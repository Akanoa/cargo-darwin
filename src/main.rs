use cargo_darwin::run;

fn main() {
    env_logger::init();
    if let Err(report) = run() {
        let _ = dbg!(report);
    }
}
