use anyhow::{Context, Result};
use std::process::Command;

const PROGRAM: &'static str = "ffmpeg";

pub fn check_ffmpeg() -> Result<bool> {
    let output = Command::new(PROGRAM)
        .arg("-version")
        .output()
        .with_context(|| format!("Please install {} with `brew install {}`", PROGRAM, PROGRAM))?;

    if !String::from_utf8(output.stdout.to_vec())
        .with_context(|| format!("Fail to parse `{} -version`", PROGRAM))?
        .contains("--enable-libx264")
    {
        anyhow::bail!("ffmpeg does not support codec 'libx264', please reinstall with the option '--enable-libx264'")
    }
    Ok(true)
}

pub fn generate_with_ffmpeg(input: &str, output: &str) -> Result<()> {
    Command::new(PROGRAM)
        .arg("-i")
        .arg(input)
        .arg("-vf")
        // fixed: `scale=(iw/2)*2:(ih/2)*2`, when picture's  width or height is odd would be error
        .arg("format=yuv420p,scale=(iw/2)*2:(ih/2)*2")
        .arg("-c:v")
        .arg("libx264")
        .arg("-y")
        .arg(output)
        .output()
        .with_context(|| format!("Fail generate video with {}", PROGRAM))
        .map(|_| ())
}

#[inline]
pub fn clear_screen() {
    print!("\x1b[2J\x1b[H");
}
