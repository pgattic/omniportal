#![cfg_attr(target_arch = "xtensa", no_std)]

extern crate alloc;

pub mod config;
pub mod domain;
pub mod figures;
pub mod state;
pub mod storage;
pub mod usb;
pub mod web;

#[cfg(target_arch = "xtensa")]
pub mod dhcp;
#[cfg(target_arch = "xtensa")]
pub mod platform;
