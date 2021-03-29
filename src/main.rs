mod common;
mod macos;

use anyhow::{Context, Result};
use gifski::{Collector, Settings};
use std::ops::{Div, Mul};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::common::{check_ffmpeg, generate_with_ffmpeg, clear_screen};
use crate::macos::capture::capture_by_id;
use gifski::progress::NoProgress;
use std::fs::File;
use std::io::BufWriter;
use std::process::Command;

pub type WinIdList = Vec<(Option<String>, u32)>;

// Two version name: English and Chinese
const TERMINAL: &[&str] = &["Terminal", "终端"];


fn main() -> Result<()> {
    let program = option_env!("CARGO_PKG_NAME").unwrap();
    let args: Vec<_> = std::env::args().collect();
    let mut options = getopts::Options::new();
    options.optflag("h", "help", "Show this usage");
    options.optflag("v", "video", "Try to generate video");
    options.optflag("l", "ls", "list window name and id");

    options.optopt(
        "",
        "fmt",
        "The output video format.[default: mp4]",
        "",
    );
    options.optopt(
        "",
        "name",
        &format!("Output file name. [default: {}]", program),
        "",
    );
    options.optopt(
        "",
        "id",
        &format!("Input window id, using {} -l to look out window id", program),
        "",
    );
    options.optopt("r", "fps", "Gif fps. [default: 5]", "");

    let parser = options.parse(&args[1..]).context("Fail to parse args")?;
    if parser.opt_present("help") {
        let version = option_env!("CARGO_PKG_VERSION").unwrap();
        let brief = format!(
            "{} {}\nUsage: {} [OPTIONS] [SHELL]",
            program, version, program
        );
        println!("{}", options.usage(&brief));
        return Ok(());
    }

    // just list window name and id, then return to terminate program
    if parser.opt_present("ls") {
        return list_win();
    }

    let video = parser.opt_present("video");
    let format = parser.opt_str("fmt").unwrap_or("mp4".to_string());
    let filename = {
        let ori_name = parser.opt_str("name").unwrap_or("wingif".to_string());
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)
            .context("Fail to get system time")?
            .as_secs();
        format!("{}_{}", ori_name, timestamp)
    };

    let fps: u8 = parser.opt_str("fps").unwrap_or("5".to_string()).parse()?;

    let win_id: u32 = if let Some(val) = parser.opt_str("id") {
        val.parse()
            .context("Fail to parse WinID")?
    } else {
        use crate::macos::window_list;
        let win_list = window_list()?;
        let mut res = None;
        for name in TERMINAL {
            if let Some(val) = get_id_for_name(&win_list, name) {
                res = Some(val);
                break;
            }
        }
        res.context("Fail to get current terminal id")?
    };

    let shell = if !parser.free.is_empty() {
        parser.free[0].clone()
    } else {
        option_env!("SHELL")
            .context("Can't get shell name")?
            .to_string()
    };

    let test_img = capture_by_id(win_id)?;
    let (width, height) = (test_img.width() as u32, test_img.height() as u32);
    let mut setting = Settings::default();
    setting.width = Some(width);
    setting.height = Some(height);

    let (collector, writer) = gifski::new(setting).context("Fail to init gifski code")?;
    let (tx, rx) = mpsc::channel();
    let mut gif_name = filename.clone();
    gif_name.push_str(".gif");
    let buf_writer = BufWriter::new(
        File::create(&gif_name).context(format!("Fail to create out file {}", filename))?,
    );

    let sub_shell = std::thread::spawn(move || {
        Command::new(&shell)
            .spawn()
            .context(format!("Fail spawn a child shell `{}`", &shell))?
            .wait()
            .context("Fail to wait child shell")
    });
    clear_screen();
    println!("Ctrl+D to terminate record");

    // wait for the terminal init
    std::thread::sleep(Duration::from_millis(250));

    let recorder_thread = std::thread::spawn(move || capture_thread(rx, collector, win_id, fps));
    let writer_thread = std::thread::spawn(move || writer.write(buf_writer, &mut NoProgress {}));

    sub_shell.join().unwrap().context("Can't launch program")?;
    tx.send(()).context("Fail to terminate sub process")?;
    recorder_thread
        .join()
        .unwrap()
        .context("Can't launch recorder process")?;
    writer_thread
        .join()
        .unwrap()
        .context("Fail to write file")?;
    println!("Write out Gif file done");

    if video && check_ffmpeg()? {
        let mut video_name = filename.clone();
        video_name.push('.');
        video_name.push_str(&format);
        println!("Converting Gif to Video🎬");
        generate_with_ffmpeg(&gif_name, &video_name)?;
    }
    Ok(())
}

fn capture_thread(rx: Receiver<()>, mut codec: Collector, win_id: u32, fps: u8) -> Result<()> {
    let mut frame_idx = 0;
    let interval_time = Duration::from_millis(1000.0.mul(1.0.div(fps as f32)) as u64);
    let min_interval = Duration::from_millis(1);
    let test_img = capture_by_id(win_id)?;
    let (w, h) = (test_img.width(), test_img.height());

    loop {
        let start = Instant::now();
        let pts = frame_idx as f64 / fps as f64;
        let frame = capture_by_id(win_id)?;
        if w != frame.width() || h != frame.height() {
            continue;
        }
        codec
            .add_frame_rgba(frame_idx, frame, pts)
            .with_context(|| format!("Fail to encode frame {}", frame_idx))?;
        let pass_t = start.elapsed();
        if (interval_time > pass_t && rx.recv_timeout(interval_time - pass_t).is_ok())
            || rx.recv_timeout(min_interval).is_ok()
        {
            break;
        }
        frame_idx += 1;
    }
    Ok(())
}

fn list_win() -> Result<()> {
    use crate::macos::window_list;

    for (name, id) in window_list()?.drain(..) {
        let name = name.unwrap_or("\t".to_string());
        println!("{} | {}", name, id);
    }
    Ok(())
}

fn get_id_for_name(win_list: &WinIdList, name: &str) -> Option<u32> {
    for (inner, id) in win_list {
        if let Some(val) = inner {
            if val.to_ascii_lowercase().contains(name) {
                return Some(*id);
            }
        }
    }
    None
}
