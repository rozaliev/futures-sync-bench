use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use futures::prelude::*;
use futures::task::{Wake, Waker};

use executor::{ExecutorHandle, ExecutorParams};

pub struct InitFuture<P: ExecutorParams> {
    pub ring_size: u32,
    pub last: Rc<RefCell<Waker>>,
    pub spawned: bool,
    pub eh: ExecutorHandle<P>,
    pub root: Rc<RefCell<Waker>>,
}

pub struct RootRingFuture {
    pub root: Rc<RefCell<Waker>>,
    pub last: Waker,
    pub fired: bool,
}

struct RingFuture<P: ExecutorParams> {
    index: u32,
    ring_size: u32,
    prev: Waker,
    root: Rc<RefCell<Waker>>,
    last: Rc<RefCell<Waker>>,
    spawned: bool,
    eh: ExecutorHandle<P>,
}

impl Future for RootRingFuture {
    type Item = ();
    type Error = Never;

    fn poll(&mut self, cx: &mut task::Context) -> Poll<Self::Item, Self::Error> {
        if self.fired {
            return Ok(Async::Ready(()));
        }

        *self.root.borrow_mut() = cx.waker().clone();
        self.last.wake();
        self.fired = true;
        Ok(Async::Pending)
    }
}

impl<P: ExecutorParams> Future for InitFuture<P> {
    type Item = ();
    type Error = Never;

    fn poll(&mut self, cx: &mut task::Context) -> Poll<Self::Item, Self::Error> {
        if !self.spawned {
            let w = cx.waker().clone();
            let mut eh = self.eh.clone();
            *self.root.borrow_mut() = w.clone();
            eh.spawn_local(RingFuture {
                root: self.root.clone(),
                index: 1,
                ring_size: self.ring_size,
                prev: w,
                spawned: false,
                last: self.last.clone(),
                eh: self.eh.clone(),
            }).unwrap();
            self.spawned = true;
            return Ok(Async::Pending);
        }

        Ok(Async::Ready(()))
    }
}

impl<P: ExecutorParams> Future for RingFuture<P> {
    type Item = ();
    type Error = Never;

    fn poll(&mut self, cx: &mut task::Context) -> Poll<Self::Item, Self::Error> {
        if self.spawned {
            if self.index == 1 {
                self.root.borrow().wake();
            } else {
                self.prev.wake();
            }

            return Ok(Async::Pending);
        }

        if self.index >= self.ring_size {
            *self.last.borrow_mut() = cx.waker().clone();
            self.root.borrow_mut().wake();
        } else {
            let w = cx.waker().clone();
            let mut eh = self.eh.clone();
            eh.spawn_local(RingFuture {
                root: self.root.clone(),
                index: self.index + 1,
                ring_size: self.ring_size,
                prev: w,
                spawned: false,
                last: self.last.clone(),
                eh: self.eh.clone(),
            }).unwrap();
        }
        self.spawned = true;

        Ok(Async::Pending)
    }
}

pub struct NoopWaker;

impl Wake for NoopWaker {
    fn wake(_arc_self: &Arc<Self>) {}
}

impl NoopWaker {
    pub fn new() -> Waker {
        Arc::new(NoopWaker).into()
    }
}
