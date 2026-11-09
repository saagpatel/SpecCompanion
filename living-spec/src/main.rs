fn main() {
    if let Err(error) = living_spec::cli::run(&std::env::args().skip(1).collect::<Vec<_>>()) {
        eprintln!("{error}");
        std::process::exit(error.code());
    }
}
