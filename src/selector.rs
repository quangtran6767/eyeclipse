use anyhow::{Context, Result};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::*;
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::COPY_DEPTH_FROM_PARENT;

use crate::capture;

#[derive(Debug, Clone, Copy)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

struct SelectionState {
    start_x: i16,
    start_y: i16,
    current_x: i16,
    current_y: i16,
    dragging: bool,
}

impl SelectionState {
    fn normalized(&self, max_w: u16, max_h: u16) -> (i16, i16, u16, u16) {
        let x = self.start_x.min(self.current_x).max(0);
        let y = self.start_y.min(self.current_y).max(0);
        let x2 = self.start_x.max(self.current_x).min(max_w as i16);
        let y2 = self.start_y.max(self.current_y).min(max_h as i16);
        let w = (x2 - x).max(0) as u16;
        let h = (y2 - y).max(0) as u16;
        (x, y, w, h)
    }
}

/// Show a fullscreen overlay and let the user draw a selection rectangle.
/// Returns `Some(Region)` on success, or `None` if the user pressed Escape.
pub fn select_region() -> Result<Option<Region>> {
    let (conn, screen_num) = RustConnection::connect(None).context("Failed to connect to X11")?;
    let screen = &conn.setup().roots[screen_num];
    let root = screen.root;
    let screen_width = screen.width_in_pixels;
    let screen_height = screen.height_in_pixels;

    // Capture the screen before showing overlay
    let bg_image = capture::capture_full_screen()?;
    let bg_rgba = bg_image.to_rgba8();

    // Apply dark tint to background
    let mut tinted = bg_rgba.clone();
    for pixel in tinted.pixels_mut() {
        pixel[0] = (pixel[0] as u16 * 120 / 255) as u8;
        pixel[1] = (pixel[1] as u16 * 120 / 255) as u8;
        pixel[2] = (pixel[2] as u16 * 120 / 255) as u8;
    }

    // Create the overlay window
    let win = conn.generate_id()?;
    let values = CreateWindowAux::new()
        .background_pixel(screen.black_pixel)
        .override_redirect(1)
        .event_mask(
            EventMask::EXPOSURE
                | EventMask::BUTTON_PRESS
                | EventMask::BUTTON_RELEASE
                | EventMask::BUTTON_MOTION
                | EventMask::POINTER_MOTION
                | EventMask::KEY_PRESS,
        );

    conn.create_window(
        COPY_DEPTH_FROM_PARENT,
        win,
        root,
        0,
        0,
        screen_width,
        screen_height,
        0,
        WindowClass::INPUT_OUTPUT,
        0,
        &values,
    )?;

    // Create GC for drawing
    let gc = conn.generate_id()?;
    conn.create_gc(gc, win, &CreateGCAux::new())?;

    // Create pixmap from tinted screenshot for background (persistent)
    let bg_pixmap = conn.generate_id()?;
    conn.create_pixmap(screen.root_depth, bg_pixmap, win, screen_width, screen_height)?;

    // Create a scratch pixmap for double-buffering (avoids tearing)
    let scratch = conn.generate_id()?;
    conn.create_pixmap(screen.root_depth, scratch, win, screen_width, screen_height)?;

    // Convert RGBA to the X11 expected BGRA format
    let mut bgra_data: Vec<u8> = Vec::with_capacity(tinted.len());
    for pixel in tinted.pixels() {
        bgra_data.push(pixel[2]); // B
        bgra_data.push(pixel[1]); // G
        bgra_data.push(pixel[0]); // R
        bgra_data.push(pixel[3]); // A
    }

    // Also prepare the original (un-tinted) BGRA for the clear region
    let mut orig_bgra_full: Vec<u8> = Vec::with_capacity(bg_rgba.len());
    for pixel in bg_rgba.pixels() {
        orig_bgra_full.push(pixel[2]);
        orig_bgra_full.push(pixel[1]);
        orig_bgra_full.push(pixel[0]);
        orig_bgra_full.push(pixel[3]);
    }

    // Also create a pixmap for the original un-tinted image
    let orig_pixmap = conn.generate_id()?;
    conn.create_pixmap(screen.root_depth, orig_pixmap, win, screen_width, screen_height)?;

    // Put the image data in chunks (X11 has request size limits)
    let bytes_per_row = screen_width as usize * 4;
    let max_rows_per_request = 8192;

    // Upload tinted to bg_pixmap
    let mut y_offset = 0u16;
    while y_offset < screen_height {
        let rows = max_rows_per_request.min((screen_height - y_offset) as usize);
        let start = y_offset as usize * bytes_per_row;
        let end = start + rows * bytes_per_row;
        if end > bgra_data.len() {
            break;
        }
        conn.put_image(
            ImageFormat::Z_PIXMAP,
            bg_pixmap,
            gc,
            screen_width,
            rows as u16,
            0,
            y_offset as i16,
            0,
            screen.root_depth,
            &bgra_data[start..end],
        )?;
        y_offset += rows as u16;
    }

    // Upload original to orig_pixmap
    y_offset = 0;
    while y_offset < screen_height {
        let rows = max_rows_per_request.min((screen_height - y_offset) as usize);
        let start = y_offset as usize * bytes_per_row;
        let end = start + rows * bytes_per_row;
        if end > orig_bgra_full.len() {
            break;
        }
        conn.put_image(
            ImageFormat::Z_PIXMAP,
            orig_pixmap,
            gc,
            screen_width,
            rows as u16,
            0,
            y_offset as i16,
            0,
            screen.root_depth,
            &orig_bgra_full[start..end],
        )?;
        y_offset += rows as u16;
    }

    // Free the large buffers now that they're in pixmaps
    drop(bgra_data);
    drop(orig_bgra_full);

    conn.map_window(win)?;
    conn.flush()?;

    // Grab keyboard and pointer
    conn.grab_keyboard(
        true,
        win,
        x11rb::CURRENT_TIME,
        GrabMode::ASYNC,
        GrabMode::ASYNC,
    )?;
    conn.grab_pointer(
        true,
        win,
        (EventMask::BUTTON_PRESS
            | EventMask::BUTTON_RELEASE
            | EventMask::BUTTON_MOTION
            | EventMask::POINTER_MOTION)
            .into(),
        GrabMode::ASYNC,
        GrabMode::ASYNC,
        win,
        0u32,
        x11rb::CURRENT_TIME,
    )?;
    conn.flush()?;

    let mut state = SelectionState {
        start_x: 0,
        start_y: 0,
        current_x: 0,
        current_y: 0,
        dragging: false,
    };

    #[allow(unused_assignments)]
    let mut result: Option<Region> = None;

    // Create a GC for the selection rectangle (white XOR)
    let sel_gc = conn.generate_id()?;
    conn.create_gc(
        sel_gc,
        win,
        &CreateGCAux::new()
            .foreground(screen.white_pixel)
            .function(GX::XOR)
            .line_width(2)
            .subwindow_mode(SubwindowMode::INCLUDE_INFERIORS),
    )?;

    // Create GC for crosshair
    let cross_gc = conn.generate_id()?;
    conn.create_gc(
        cross_gc,
        win,
        &CreateGCAux::new()
            .foreground(screen.white_pixel)
            .function(GX::XOR)
            .line_width(1)
            .line_style(LineStyle::ON_OFF_DASH),
    )?;

    loop {
        let event = conn.wait_for_event()?;
        match event {
            Event::Expose(_) => {
                // Draw the tinted background
                conn.copy_area(bg_pixmap, win, gc, 0, 0, 0, 0, screen_width, screen_height)?;
                conn.flush()?;
            }
            Event::KeyPress(ev) => {
                // Escape key = keycode 9
                if ev.detail == 9 {
                    result = None;
                    break;
                }
            }
            Event::ButtonPress(ev) => {
                if ev.detail == 1 {
                    state.start_x = ev.event_x;
                    state.start_y = ev.event_y;
                    state.current_x = ev.event_x;
                    state.current_y = ev.event_y;
                    state.dragging = true;
                }
            }
            Event::MotionNotify(ev) => {
                let mx = ev.event_x.max(0).min(screen_width as i16 - 1);
                let my = ev.event_y.max(0).min(screen_height as i16 - 1);

                // Start with tinted background on the scratch pixmap
                conn.copy_area(bg_pixmap, scratch, gc, 0, 0, 0, 0, screen_width, screen_height)?;

                if state.dragging {
                    state.current_x = mx;
                    state.current_y = my;
                    let (rx, ry, rw, rh) = state.normalized(screen_width, screen_height);

                    if rw > 0 && rh > 0 {
                        // Copy the un-tinted region from orig_pixmap onto scratch
                        conn.copy_area(
                            orig_pixmap, scratch, gc,
                            rx, ry,       // src x, y
                            rx, ry,       // dst x, y
                            rw, rh,
                        )?;

                        // Draw selection rectangle border on scratch
                        conn.poly_rectangle(scratch, sel_gc, &[Rectangle {
                            x: rx,
                            y: ry,
                            width: rw,
                            height: rh,
                        }])?;

                        // Draw dimension label
                        let label = format!("{}x{}", rw, rh);
                        let label_x = rx + 4;
                        let label_y = if ry < 18 { ry + rh as i16 + 14 } else { ry - 4 };
                        conn.image_text8(scratch, sel_gc, label_x, label_y, label.as_bytes())?;
                    }
                } else {
                    // Draw crosshair at cursor position on scratch
                    conn.poly_segment(scratch, cross_gc, &[
                        Segment { x1: mx, y1: 0, x2: mx, y2: screen_height as i16 },
                        Segment { x1: 0, y1: my, x2: screen_width as i16, y2: my },
                    ])?;
                }

                // Single
                conn.copy_area(scratch, win, gc, 0, 0, 0, 0, screen_width, screen_height)?;
                conn.flush()?;
            }
            Event::ButtonRelease(ev) => {
                if ev.detail == 1 && state.dragging {
                    state.current_x = ev.event_x.max(0).min(screen_width as i16 - 1);
                    state.current_y = ev.event_y.max(0).min(screen_height as i16 - 1);
                    state.dragging = false;

                    let (rx, ry, rw, rh) = state.normalized(screen_width, screen_height);
                    if rw > 2 && rh > 2 {
                        result = Some(Region {
                            x: rx as i32,
                            y: ry as i32,
                            width: rw as u32,
                            height: rh as u32,
                        });
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    // Cleanup
    conn.ungrab_pointer(x11rb::CURRENT_TIME)?;
    conn.ungrab_keyboard(x11rb::CURRENT_TIME)?;
    conn.free_gc(sel_gc)?;
    conn.free_gc(cross_gc)?;
    conn.free_gc(gc)?;
    conn.free_pixmap(bg_pixmap)?;
    conn.free_pixmap(orig_pixmap)?;
    conn.free_pixmap(scratch)?;
    conn.destroy_window(win)?;
    conn.flush()?;

    Ok(result)
}
