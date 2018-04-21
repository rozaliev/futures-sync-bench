use super::{ExecutorParams, WakeReceiver, WakeSender};
use crossbeam_channel::{unbounded, Receiver, Sender};

pub struct CrossbeamUnboundedChannelImpl;

impl ExecutorParams for CrossbeamUnboundedChannelImpl {
    type WakeReceiver = Receiver<u64>;
    type WakeSender = Sender<u64>;

    fn pair() -> (Self::WakeSender, Self::WakeReceiver) {
        unbounded()
    }
}

impl WakeSender for Sender<u64> {
    fn push(&self, msg: u64) {
        self.send(msg).unwrap();
    }
}

impl WakeReceiver for Receiver<u64> {
    fn pop(&mut self) -> Option<u64> {
        self.try_recv().ok()
    }
}
