extern crate actix_web;
extern crate dirs;
extern crate failure;
extern crate fern;
extern crate log;
extern crate parking_lot;

use actix_web::{web, App, HttpServer};
use failure::Error;
use std::process::Child;
use std::sync::Arc;

use parking_lot::Mutex;
use serde::Deserialize;

struct Brightness {
    value: f32,
}

impl Brightness {
    fn to_light(&self) -> f32 {
        if self.value < 100.10673f32 {
            0.10673
        } else {
            self.value - 100f32
        }
    }

    fn to_redshift(&self) -> f32 {
        if self.value > 100f32 {
            1.0
        } else {
            self.value / 100.0
        }
    }

    fn change(&mut self, amount: f32) {
        self.set(self.value + amount);
    }

    fn set(&mut self, value: f32) {
        self.value = value.max(10.0).min(200.0);
    }
}

struct AppData {
    brightness: Brightness,
    redshift_process: Child,
}

impl AppData {
    fn kill_child(&mut self) {
        if let Err(err) = self.redshift_process.kill() {
            eprintln!("Could not kill redshift process: {}", err);
        }
    }

    fn restart(&mut self) {
        self.kill_child();
        run_light(self.brightness.to_light()).unwrap();

        let child = run_redshift(self.brightness.to_redshift()).unwrap();
        self.redshift_process = child;
    }
}

fn run_light(brightness: f32) -> Result<(), Error> {
    std::process::Command::new("light")
        .arg("-S")
        .arg(format!("{}", brightness))
        .spawn()?
        .wait()?;

    Ok(())
}

fn run_redshift(brightness: f32) -> Result<std::process::Child, Error> {
    let child = std::process::Command::new("redshift")
        .arg("-m")
        .arg("wayland")
        .arg("-O")
        .arg("6500")
        .arg("-b")
        .arg(format!("{}", brightness))
        .spawn()?;

    Ok(child)
}

fn get_screen_brightness() -> Result<Brightness, failure::Error> {
    let output = std::process::Command::new("light").output()?.stdout;
    let output_str = std::str::from_utf8(&output)?;
    println!("Light output: {}", output_str);
    let result: f32 = output_str.trim().parse()?;

    Ok(Brightness {
        value: result + 100.0,
    })
}

#[derive(Deserialize)]
struct Request {
    brightness: f32,
}

fn set_handler(req: web::Query<Request>, data: web::Data<AppState>) -> Result<(), Error> {
    data.data.lock().brightness.set(req.brightness);
    data.data.lock().restart();
    Ok(())
}

fn get_handler(data: web::Data<AppState>) -> Result<String, Error> {
    Ok(format!("{}", data.data.lock().brightness.value))
}

fn brighter_handler(data: web::Data<AppState>) -> Result<(), Error> {
    data.data.lock().brightness.change(5.0);
    data.data.lock().restart();
    Ok(())
}

fn darker_handler(data: web::Data<AppState>) -> Result<(), Error> {
    data.data.lock().brightness.change(-5.0);
    data.data.lock().restart();
    Ok(())
}

struct AppState {
    data: Arc<Mutex<AppData>>,
}

fn main() {
    let home = dirs::home_dir()
        .unwrap()
        .into_os_string()
        .into_string()
        .unwrap();

    // Configure logger at runtime
    fern::Dispatch::new()
        // Perform allocation-free log formatting
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{}][{}] {}",
                record.target(),
                record.level(),
                message
            ))
        })
        .level(log::LevelFilter::Debug)
        .chain(std::io::stdout())
        .chain(fern::log_file(format!("{}/sunset.log", home)).expect("could not set up log file"))
        .apply()
        .expect("Could not initialize logging");

    let app_state = web::Data::new(AppState {
        data: Arc::new(Mutex::new(AppData {
            brightness: get_screen_brightness().expect("Could not invoke 'light' command"),
            redshift_process: run_redshift(1.0).expect("Could not launch redshift"),
        })),
    });

    println!(
        "Initial brightness value: {}",
        app_state.data.lock().brightness.value
    );

    HttpServer::new(move || {
        App::new()
            .register_data(app_state.clone())
            .route("/get", web::get().to(get_handler))
            .route("/set", web::get().to(set_handler))
            .route("/brighter", web::get().to(brighter_handler))
            .route("/darker", web::get().to(darker_handler))
    })
    .bind("0.0.0.0:12321")
    .expect("Can not bind to port 12321")
    .run()
    .expect("Could not run web server");
}
