use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader};

fn main() {
    load_dotenv();
    tauri_build::build();
}

fn load_dotenv() {
    let path = ".env";
    let file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return,
    };

    for line in BufReader::new(file).lines().flatten() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(2, '=');
        let key = parts.next().map(str::trim);
        let value = parts.next().map(str::trim).map(|v| v.trim_matches('"'));
        if let (Some(key), Some(value)) = (key, value) {
            match key {
                "GOOGLE_CLIENT_ID" | "GOOGLE_CLIENT_SECRET" | "FIREBASE_API_KEY" => {
                    if env::var(key).is_err() {
                        println!("cargo:rustc-env={key}={value}");
                    }
                }
                _ => {}
            }
        }
    }
}
