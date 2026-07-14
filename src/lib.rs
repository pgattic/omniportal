#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod config;
#[cfg(not(test))]
pub mod dhcp;
pub mod figures;
#[cfg(not(test))]
pub mod platform;
pub mod state;
pub mod storage;
pub mod usb;
pub mod web;
