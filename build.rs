fn main() {
    if let Err(error) = slint_build::compile("ui/appwindow.slint") {
        eprintln!("failed to compile Slint UI: {error}");
        std::process::exit(1);
    }
}
