use std::collections::VecDeque;

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
        ((x), (y), (x2 - x).max(0) as u16, (y2 - y).max(0) as u16)
    }
}

/// Show a fullscreen overlay and let the user draw a selection rectangle.
/// Returns `Some(Region)` on success, or `None` if the user pressed Escape.
pub fn select_region() -> Result<Option<Region>> {
    let (conn, screen_num) = RustConnection::connect(None).context("Failed to connect to X11")?;
    let screen = &conn.setup().roots[screen_num];
    let root = screen.root;
    let sw = screen.width_in_pixels;
    let sh = screen.height_in_pixels;
    let depth = screen.root_depth;

    // Capture the screen before showing overlay
    let bg_image = capture::capture_full_screen()?;
    let bg_rgba = bg_image.to_rgba8();

    // Upload pixmaps on the root drawable (before window exists)
    let bg_pixmap = conn.generate_id()?;
    conn.create_pixmap(depth, bg_pixmap, root, sw, sh)?;
    let orig_pixmap = conn.generate_id()?;
    conn.create_pixmap(depth, orig_pixmap, root, sw, sh)?;

    let tmp_gc = conn.generate_id()?;
    conn.create_gc(tmp_gc, root, &CreateGCAux::new())?;

    // Upload original (un-tinted) first
    upload_pixmap(&conn, &bg_rgba, orig_pixmap, tmp_gc, sw, sh, depth)?;

    // Tint and upload as background
    let mut tinted = bg_rgba;
    for pixel in tinted.pixels_mut() {
        pixel[0] = (pixel[0] as u16 * 120 / 255) as u8;
        pixel[1] = (pixel[1] as u16 * 120 / 255) as u8;
        pixel[2] = (pixel[2] as u16 * 120 / 255) as u8;
    }
    upload_pixmap(&conn, &tinted, bg_pixmap, tmp_gc, sw, sh, depth)?;
    drop(tinted);
    conn.free_gc(tmp_gc)?;

    // Create window with bg_pixmap as background — X server paints it
    // atomically on map, no black flash, no tearing on startup.
    let win = conn.generate_id()?;
    conn.create_window(
        COPY_DEPTH_FROM_PARENT,
        win,
        root,
        0,
        0,
        sw,
        sh,
        0,
        WindowClass::INPUT_OUTPUT,
        0,
        &CreateWindowAux::new()
            .background_pixmap(bg_pixmap)
            .override_redirect(1)
            .event_mask(
                EventMask::EXPOSURE
                    | EventMask::BUTTON_PRESS
                    | EventMask::BUTTON_RELEASE
                    | EventMask::BUTTON_MOTION
                    | EventMask::POINTER_MOTION
                    | EventMask::KEY_PRESS,
            ),
    )?;

    // GCs
    let gc = conn.generate_id()?;
    conn.create_gc(gc, win, &CreateGCAux::new())?;

    let sel_gc = conn.generate_id()?;
    conn.create_gc(
        sel_gc,
        win,
        &CreateGCAux::new()
            .foreground(screen.white_pixel)
            .function(GX::XOR)
            .line_width(2),
    )?;

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

    conn.map_window(win)?;
    conn.flush()?;

    // Grab keyboard and pointer
    conn.grab_keyboard(true, win, x11rb::CURRENT_TIME, GrabMode::ASYNC, GrabMode::ASYNC)?;
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

    // Track previous drawings for efficient dirty-rect erasure
    let mut prev_cross: Option<(i16, i16)> = None;
    let mut prev_sel: Option<(i16, i16, u16, u16)> = None;
    let mut pending: VecDeque<Event> = VecDeque::new();

    loop {
        let event = pending
            .pop_front()
            .map(Ok)
            .unwrap_or_else(|| conn.wait_for_event())?;

        match event {
            Event::Expose(_) => {
                // X server repaints background from bg_pixmap automatically.
                // Just redraw the active overlay on top.
                if let Some((rx, ry, rw, rh)) = prev_sel {
                    if rw > 0 && rh > 0 {
                        conn.copy_area(orig_pixmap, win, gc, rx, ry, rx, ry, rw, rh)?;
                        draw_sel(&conn, win, sel_gc, rx, ry, rw, rh)?;
                    }
                } else if let Some((cx, cy)) = prev_cross {
                    draw_cross(&conn, win, cross_gc, cx, cy, sw, sh)?;
                }
                conn.flush()?;
            }
            Event::KeyPress(ev) => {
                if ev.detail == 9 {
                    result = None;
                    break;
                }
            }
            Event::ButtonPress(ev) if ev.detail == 1 => {
                // Erase crosshair before starting drag
                if let Some((ox, oy)) = prev_cross.take() {
                    draw_cross(&conn, win, cross_gc, ox, oy, sw, sh)?;
                }
                let mx = ev.event_x.max(0).min(sw as i16 - 1);
                let my = ev.event_y.max(0).min(sh as i16 - 1);
                state.start_x = mx;
                state.start_y = my;
                state.current_x = mx;
                state.current_y = my;
                state.dragging = true;
                conn.flush()?;
            }
            Event::MotionNotify(ev) => {
                let mut mx = ev.event_x.max(0).min(sw as i16 - 1);
                let mut my = ev.event_y.max(0).min(sh as i16 - 1);

                // Coalesce queued motion events — only render the latest position
                while let Some(q) = conn.poll_for_event()? {
                    if let Event::MotionNotify(m) = &q {
                        mx = m.event_x.max(0).min(sw as i16 - 1);
                        my = m.event_y.max(0).min(sh as i16 - 1);
                    } else {
                        pending.push_back(q);
                        break;
                    }
                }

                if state.dragging {
                    state.current_x = mx;
                    state.current_y = my;
                    let (rx, ry, rw, rh) = state.normalized(sw, sh);

                    // Erase previous selection — only the small dirty rect, NOT full screen
                    if let Some((ox, oy, ow, oh)) = prev_sel.take() {
                        erase_dirty(&conn, bg_pixmap, win, gc, ox, oy, ow, oh, sw, sh)?;
                    }

                    if rw > 0 && rh > 0 {
                        // Copy un-tinted region (small rect, not full screen)
                        conn.copy_area(orig_pixmap, win, gc, rx, ry, rx, ry, rw, rh)?;
                        draw_sel(&conn, win, sel_gc, rx, ry, rw, rh)?;
                        prev_sel = Some((rx, ry, rw, rh));
                    }
                } else {
                    // XOR erase old crosshair + XOR draw new (just 4 line draws total)
                    if let Some((ox, oy)) = prev_cross.take() {
                        draw_cross(&conn, win, cross_gc, ox, oy, sw, sh)?;
                    }
                    draw_cross(&conn, win, cross_gc, mx, my, sw, sh)?;
                    prev_cross = Some((mx, my));
                }

                conn.flush()?;
            }
            Event::ButtonRelease(ev) if ev.detail == 1 && state.dragging => {
                state.current_x = ev.event_x.max(0).min(sw as i16 - 1);
                state.current_y = ev.event_y.max(0).min(sh as i16 - 1);
                state.dragging = false;

                let (rx, ry, rw, rh) = state.normalized(sw, sh);
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
    conn.destroy_window(win)?;
    conn.flush()?;

    Ok(result)
}

/// XOR draw/erase crosshair (calling twice at same position erases it).
fn draw_cross(
    conn: &RustConnection, win: Window, gc: Gcontext,
    x: i16, y: i16, sw: u16, sh: u16,
) -> Result<()> {
    conn.poly_segment(win, gc, &[
        Segment { x1: x, y1: 0, x2: x, y2: sh as i16 },
        Segment { x1: 0, y1: y, x2: sw as i16, y2: y },
    ])?;
    Ok(())
}

/// Draw selection rectangle border + dimension label.
fn draw_sel(
    conn: &RustConnection, d: Drawable, gc: Gcontext,
    rx: i16, ry: i16, rw: u16, rh: u16,
) -> Result<()> {
    conn.poly_rectangle(d, gc, &[Rectangle { x: rx, y: ry, width: rw, height: rh }])?;
    let label = format!("{}x{}", rw, rh);
    let lx = rx + 4;
    let ly = if ry < 18 { ry + rh as i16 + 14 } else { ry - 4 };
    conn.image_text8(d, gc, lx, ly, label.as_bytes())?;
    Ok(())
}

/// Restore tinted background over a dirty rect (selection + border + label padding).
fn erase_dirty(
    conn: &RustConnection, bg: Pixmap, win: Window, gc: Gcontext,
    ox: i16, oy: i16, ow: u16, oh: u16, sw: u16, sh: u16,
) -> Result<()> {
    // Pad for XOR border width (2px) + label text (~14px high, ~80px wide)
    let ex = (ox - 3).max(0);
    let ey = (oy - 20).max(0);
    let ex2 = ((ox as i32 + ow as i32 + 3).min(sw as i32)) as i16;
    let ey2 = ((oy as i32 + oh as i32 + 20).min(sh as i32)) as i16;
    let ew = (ex2 - ex).max(0) as u16;
    let eh = (ey2 - ey).max(0) as u16;
    if ew > 0 && eh > 0 {
        conn.copy_area(bg, win, gc, ex, ey, ex, ey, ew, eh)?;
    }
    Ok(())
}

/// Upload an RgbaImage to an X11 pixmap (RGBA → BGRA conversion + chunked PutImage).
fn upload_pixmap(
    conn: &RustConnection, img: &image::RgbaImage, pixmap: Pixmap,
    gc: Gcontext, sw: u16, sh: u16, depth: u8,
) -> Result<()> {
    let bpr = sw as usize * 4;
    let max_rows = 8192usize;
    let mut bgra: Vec<u8> = Vec::with_capacity(img.len());
    for p in img.pixels() {
        bgra.push(p[2]);
        bgra.push(p[1]);
        bgra.push(p[0]);
        bgra.push(p[3]);
    }
    let mut y = 0u16;
    while y < sh {
        let rows = max_rows.min((sh - y) as usize);
        let start = y as usize * bpr;
        let end = start + rows * bpr;
        if end > bgra.len() {
            break;
        }
        conn.put_image(
            ImageFormat::Z_PIXMAP, pixmap, gc, sw, rows as u16,
            0, y as i16, 0, depth, &bgra[start..end],
        )?;
        y += rows as u16;
    }
    Ok(())
}
