#![feature(test)]
#![feature(integer_atomics)]

extern crate crossbeam_channel;
extern crate futures;
extern crate slab;
extern crate test;

mod cases;
mod executor;
