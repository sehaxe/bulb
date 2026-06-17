//! Configuration loading.
//!
//! Currently only [`pacman_conf`] is implemented — reading `/etc/pacman.conf`
//! so bulb can act as a drop-in. A native `bulb.conf` (TOML) will live here
//! later as `bulb_conf`.

pub mod pacman_conf;

pub use pacman_conf::{Options, PacmanConf, Repo};
