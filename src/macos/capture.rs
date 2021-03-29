use rgb::{RGBA8, RGBA};
use imgref::{ImgVec, Img};
use core_graphics::display::*;
use anyhow::{Context, Result};
use rayon::slice::ParallelSlice;
use rayon::iter::ParallelIterator;


pub fn capture_by_id(win_id: u32) -> Result<ImgVec<RGBA8>> {
    let screenshot = unsafe {
        CGDisplay::screenshot(
            CGRectNull,
            kCGWindowListOptionIncludingWindow | kCGWindowListExcludeDesktopElements,
            win_id,
            kCGWindowImageNominalResolution
                | kCGWindowImageBoundsIgnoreFraming
                | kCGWindowImageShouldBeOpaque
        )
    }.context(format!("Fail to capture window id: {}", win_id))?;

    let screen_ref = screenshot.as_ref();
    let h = screen_ref.height() as usize;
    let bytes_per_row = screen_ref.bytes_per_row() as usize;
    let bits_per_pixel = screen_ref.bits_per_pixel() as usize;

    // caution: call method `width()` to get width is no correct
    let w = bytes_per_row / bits_per_pixel * 8;

    let img_data = screen_ref.data().to_vec();
    // using rayon's parallel to accelerate transform
    let img_rgba: Vec<_> = img_data.par_chunks(4)
        .map(|pixels| RGBA::new(pixels[2], pixels[1], pixels[0], pixels[3]))
        .collect();

    debug_assert_eq!(img_rgba.len(), w * h);

    Ok(Img::new(img_rgba, w, h))
}