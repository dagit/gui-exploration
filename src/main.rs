// Based on this example C code:
//   https://github.com/ereslibre/x11/blob/master/xrender/rendertext.c
use std::ffi::CString;
use std::mem;
use std::os::raw::*;
use std::ptr;

use x11::xlib;
use x11::xrender::{
    XRenderAddGlyphs, XRenderColor, XRenderCompositeString8, XRenderCreateGlyphSet,
    XRenderCreatePicture, XRenderFillRectangle, XRenderFindStandardFormat, XRenderFindVisualFormat,
    XRenderPictureAttributes,
};

fn load_glyph(
    display: *mut x11::xlib::Display,
    gs: x11::xrender::GlyphSet,
    face: &freetype::face::Face,
    charcode: usize,
) {
    let glyph_index = face.get_char_index(charcode);
    face.load_glyph(
        glyph_index,
        freetype::face::LoadFlag::RENDER.union(freetype::face::LoadFlag::FORCE_AUTOHINT),
    )
    .unwrap();
    let bitmap = &mut face.glyph().bitmap();
    let ginfo = x11::xrender::XGlyphInfo {
        x: -face.glyph().bitmap_left() as _,
        y: face.glyph().bitmap_top() as _,
        width: face.glyph().bitmap().width() as _,
        height: face.glyph().bitmap().rows() as _,
        xOff: (face.glyph().advance().x / 64) as i16,
        yOff: (face.glyph().advance().y / 64) as i16,
    };

    let gid = charcode as u64;

    let stride = (ginfo.width + 3) & !3;
    let mut tmpbitmap = vec![0u8; (stride * ginfo.height) as usize];
    for y in 0..ginfo.height {
        unsafe {
            std::ptr::copy_nonoverlapping(
                &bitmap.buffer()[(y * ginfo.width) as usize],
                &mut tmpbitmap[(y * stride) as usize],
                ginfo.width as _,
            );
        }
    }

    unsafe {
        XRenderAddGlyphs(
            display,
            gs,
            &gid,
            &ginfo,
            1,
            tmpbitmap.as_ptr() as *const i8,
            (stride * ginfo.height) as _,
        );
        x11::xlib::XSync(display, 0);
    }
}

fn load_glyphset(
    display: *mut x11::xlib::Display,
    library: &freetype::library::Library,
    size: usize,
) -> x11::xrender::GlyphSet {
    let fmt_a8 = unsafe { XRenderFindStandardFormat(display, x11::xrender::PictStandardA8) };
    let gs = unsafe { XRenderCreateGlyphSet(display, fmt_a8) };
    let font_file = include_bytes!("../assets/DejaVuSans.ttf").to_vec();
    let face = library.new_memory_face(font_file, 0).unwrap();
    face.set_char_size(0, (size * 64) as _, 90, 90).unwrap();
    for n in 32..128 {
        load_glyph(display, gs, &face, n);
    }
    gs
}

fn create_pen(
    display: *mut x11::xlib::Display,
    red: c_ushort,
    green: c_ushort,
    blue: c_ushort,
    alpha: c_ushort,
) -> x11::xrender::Picture {
    let color = XRenderColor {
        red,
        green,
        blue,
        alpha,
    };
    let fmt = unsafe { XRenderFindStandardFormat(display, x11::xrender::PictStandardARGB32) };

    let root = unsafe { x11::xlib::XDefaultRootWindow(display) };
    let pm = unsafe { x11::xlib::XCreatePixmap(display, root, 1, 1, 32) };
    let mut pict_attr: XRenderPictureAttributes = unsafe { mem::zeroed() };
    pict_attr.repeat = 1;
    let picture =
        unsafe { XRenderCreatePicture(display, pm, fmt, x11::xrender::CPRepeat as _, &pict_attr) };
    unsafe {
        XRenderFillRectangle(
            display,
            x11::xrender::PictOpOver,
            picture,
            &color,
            0,
            0,
            1,
            1,
        )
    };
    unsafe { x11::xlib::XFreePixmap(display, pm) };
    picture
}

fn main() {
    unsafe {
        // Open display connection.
        let display = xlib::XOpenDisplay(ptr::null());

        if display.is_null() {
            panic!("XOpenDisplay failed");
        }

        // The original C code found the format using:
        // XRenderFindStandardFormat(display, PictStandardRGB24);
        // For some reason that works in C but not from Rust.
        // So instead, we grab the format of the root visual as follows:
        let fmt = XRenderFindVisualFormat(
            display,
            (*x11::xlib::XDefaultScreenOfDisplay(display)).root_visual,
        );
        let screen = xlib::XDefaultScreen(display);
        let root = xlib::XDefaultRootWindow(display);

        // Create window.
        let window = xlib::XCreateWindow(
            display,
            root,
            0,
            0,
            640,
            480,
            0,
            x11::xlib::XDefaultDepth(display, screen),
            xlib::InputOutput as c_uint,
            x11::xlib::XDefaultVisual(display, screen),
            0,
            ptr::null_mut(),
        );

        let mut pict_attr: XRenderPictureAttributes = mem::zeroed();
        pict_attr.poly_edge = x11::xrender::PolyEdgeSmooth;
        pict_attr.poly_mode = x11::xrender::PolyModeImprecise;
        let picture = XRenderCreatePicture(
            display,
            window,
            fmt,
            (x11::xrender::CPPolyEdge | x11::xrender::CPPolyMode)
                .try_into()
                .unwrap(),
            &pict_attr,
        );

        use x11::xlib::{
            ButtonPressMask, ExposureMask, KeyPressMask, KeyReleaseMask, StructureNotifyMask,
        };
        x11::xlib::XSelectInput(
            display,
            window,
            KeyPressMask | KeyReleaseMask | ExposureMask | ButtonPressMask | StructureNotifyMask,
        );

        let fg_pen = create_pen(display, 0, 0, 0, 0xffff);
        let library = freetype::library::Library::init().unwrap();
        let font = load_glyphset(display, &library, 30);

        // Show window.
        xlib::XMapWindow(display, window);
        let bg_color = XRenderColor {
            red: 0xffff,
            green: 0xffff,
            blue: 0xffff,
            alpha: 0xffff,
        };

        // Set window title.
        let title_str = CString::new("hello-world").unwrap();
        xlib::XStoreName(display, window, title_str.as_ptr() as *mut c_char);

        // Hook close requests.
        let wm_protocols_str = CString::new("WM_PROTOCOLS").unwrap();
        let wm_delete_window_str = CString::new("WM_DELETE_WINDOW").unwrap();

        let wm_protocols = xlib::XInternAtom(display, wm_protocols_str.as_ptr(), xlib::False);
        let wm_delete_window =
            xlib::XInternAtom(display, wm_delete_window_str.as_ptr(), xlib::False);

        let mut protocols = [wm_delete_window];

        xlib::XSetWMProtocols(
            display,
            window,
            protocols.as_mut_ptr(),
            protocols.len() as c_int,
        );

        // Main loop.
        loop {
            let mut event: xlib::XEvent = std::mem::zeroed();
            xlib::XNextEvent(display, &mut event);

            match event.get_type() {
                xlib::ClientMessage => {
                    let xclient = xlib::XClientMessageEvent::from(event);

                    if xclient.message_type == wm_protocols && xclient.format == 32 {
                        let protocol = xclient.data.get_long(0) as xlib::Atom;

                        if protocol == wm_delete_window {
                            break;
                        }
                    }
                }
                xlib::Expose => {
                    XRenderFillRectangle(
                        display,
                        x11::xrender::PictOpOver,
                        picture,
                        &bg_color,
                        0,
                        0,
                        1640,
                        1640,
                    );
                    XRenderCompositeString8(
                        display,
                        x11::xrender::PictOpOver,
                        fg_pen,
                        picture,
                        ptr::null(),
                        font,
                        0,
                        0,
                        20,
                        50,
                        "We are jumping over a black fox".as_ptr() as *const _,
                        31,
                    );
                }
                _ => (),
            }
        }

        // Shut down.
        xlib::XCloseDisplay(display);
    }
}
