//! Android glue for the favourite launcher. Everything platform-independent lives in
//! `mivefav-core`; this crate is only built for Android.

#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "android")]
mod android_main;
