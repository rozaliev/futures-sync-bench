use std::cell::RefCell;
use std::collections::VecDeque;

use super::{ExecutorParams, WakeReceiver, WakeSender};

thread_local! {
    pub static QUEUE: RefCell<VecDeque<u64>> = RefCell::new(VecDeque::new());
}

pub struct TlsUnsafeImpl;

pub struct Receiver;
#[derive(Clone)]
pub struct Sender;

unsafe impl Send for Sender {}
unsafe impl Sync for Sender {}

impl ExecutorParams for TlsUnsafeImpl {
    type WakeReceiver = Receiver;
    type WakeSender = Sender;

    fn pair() -> (Self::WakeSender, Self::WakeReceiver) {
        (Sender, Receiver)
    }
}

impl WakeSender for Sender {
    fn push(&self, msg: u64) {
        QUEUE.with(|q| {
            q.borrow_mut().push_back(msg);
        })
    }
}

impl WakeReceiver for Receiver {
    fn pop(&mut self) -> Option<u64> {
        QUEUE.with(|q| q.borrow_mut().pop_front())
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        QUEUE.with(|q| {
            let mut q = q.borrow_mut();
            q.clear();
        })
    }
}
