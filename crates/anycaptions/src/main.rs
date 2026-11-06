#![windows_subsystem = "windows"]

mod adapters;
mod app;
mod host;
mod services;

#[cfg(test)]
mod tests;

fn main() {
    if let Err(error) = app::run() {
        eprintln!("AnyCaptions failed to start: {error}");
        std::process::exit(1);
    }
}
