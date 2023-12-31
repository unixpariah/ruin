mod battery;
mod color_schemes;

use battery::{Battery, BatteryStatus};
use color_schemes::{color_schemes, Colors};
use image::{self, imageops, io::Reader, DynamicImage, GenericImageView, ImageBuffer, Rgba};
use reqwest::get;
use std::{
    env,
    error::Error,
    fs::{self, File},
    io::{BufRead, BufReader, Cursor},
    path::PathBuf,
    process::Command,
    thread,
    time::Duration,
};

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    let name = get_name(args).expect("Could not get distribution name");
    let img_path = {
        let home_dir = env::var("HOME").expect("Could not find home dir");
        PathBuf::from(format!("{}/.ruin/{}.png", home_dir, name))
    };

    let tmp = PathBuf::from("/tmp/ruin");
    fs::create_dir_all(&tmp).expect("Failed to create tmp dir");
    let background_path = tmp.join("background.png");
    let image = match image::open(&img_path) {
        Ok(image) => image,
        Err(_) => get_image(&name, &img_path)
            .await
            .expect("Failed to fetch image from server"),
    };

    let mut previous = Battery {
        capacity: 0,
        status: BatteryStatus::NotCharging,
    };

    let color_scheme = color_schemes(&name);

    loop {
        let battery = Battery::new();
        if battery != previous {
            let image = create(&battery, &color_scheme, &image);
            let _ = set_wallpaper(image, &background_path);
            previous = battery;
        }
        thread::sleep(Duration::from_secs(5));
    }
}

fn create(
    battery: &Battery,
    color_scheme: &Colors,
    image: &DynamicImage,
) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    let (status, capacity) = (&battery.status, battery.capacity);
    let (width, height) = (image.width(), image.height());

    let color = match status {
        BatteryStatus::Charging => color_scheme.charging,
        _ if capacity >= 30_u8 => color_scheme.default,
        _ => color_scheme.low_battery,
    };

    let mut output: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(width, height);
    image.pixels().for_each(|(x, y, pixel)| {
        let capacity = 1.0 - capacity as f32 / 100.0;
        if y as f32 > height as f32 * capacity && pixel == Rgba([143, 188, 187, 255]) {
            output.put_pixel(x, y, Rgba(color));
        } else {
            output.put_pixel(x, y, pixel);
        }
    });

    let mut background = ImageBuffer::new(3840, 2160);
    background
        .pixels_mut()
        .collect::<Vec<_>>()
        .iter_mut()
        .for_each(|pixel| **pixel = Rgba(color_scheme.background));

    let x = (3840 - width) / 2;
    let y = (2160 - height) / 2;
    imageops::overlay(&mut background, &output, x as i64, y as i64);

    background
}

fn set_wallpaper(
    image: ImageBuffer<Rgba<u8>, Vec<u8>>,
    background_path: &PathBuf,
) -> Result<(), Box<dyn Error>> {
    image.save(&background_path)?;
    match env::var("XDG_SESSION_TYPE").unwrap_or_default().as_str() {
        "wayland" => {
            Command::new("swww")
                .arg("img")
                .arg(background_path)
                .spawn()?;
        }
        _ => wallpaper::set_from_path(background_path.display().to_string().as_str())?,
    }

    Ok(())
}

async fn get_image(name: &String, img_path: &PathBuf) -> Result<DynamicImage, Box<dyn Error>> {
    let image = get(format!("https://ruin.shuttleapp.rs/{name}"))
        .await?
        .bytes()
        .await?;
    let image = Reader::new(Cursor::new(image))
        .with_guessed_format()?
        .decode()?;
    image.save(img_path)?;
    Ok(image)
}

fn get_name(args: Vec<String>) -> Result<String, Box<dyn Error>> {
    if let Some(name) = args.get(1) {
        return Ok(name.into());
    }

    let file = File::open("/etc/os-release")?;
    let buf_reader = BufReader::new(file);
    let line = buf_reader
        .lines()
        .filter_map(|line| line.ok())
        .find(|line| line.split_once("=").unwrap_or_default().0 == "ID");

    Ok(line
        .ok_or("")?
        .split_once("=")
        .ok_or("")?
        .1
        .trim()
        .to_owned())
}
