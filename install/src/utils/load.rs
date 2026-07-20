use std::fs::File;
use std::io::copy;

pub fn download_file(url: &str, output: &str) -> Result<(), String> {
    let mut response = reqwest::blocking::get(url).map_err(|e| e.to_string())?;
    let mut file = File::create(output).map_err(|e| e.to_string())?;
    copy(&mut response, &mut file).map_err(|e| e.to_string())?;
    Ok(())
}
