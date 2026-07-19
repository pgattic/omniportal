#![cfg_attr(any(target_arch = "xtensa", target_arch = "arm"), no_std)]

extern crate alloc;

pub mod config;
pub mod domain;
pub mod figures;
pub mod storage;
pub mod usb;
pub mod web;

#[cfg(target_arch = "xtensa")]
pub mod dhcp;
#[cfg(any(target_arch = "xtensa", target_arch = "arm"))]
pub mod platform;
