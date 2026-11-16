//! Platform-independent parts of the favourite launcher: layout, drawing into a pixel
//! buffer, slot storage and the frame cache. Everything here builds and is tested on the host.

pub mod cache;
pub mod canvas;
pub mod icon;
pub mod layout;
pub mod slots;
pub mod text;
pub mod ui;
