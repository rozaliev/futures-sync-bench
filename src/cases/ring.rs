use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use executor::{ExecutorHandle, ExecutorParams};
use futures::prelude::*;
use futures::task::{Wake, Waker};

struct RootRingFuture<P: ExecutorParams> {
    ring_size: u32,
    iteration: u32,
    max_iterations: u32,
    last: Rc<RefCell<Waker>>,
    spawned: bool,
    eh: ExecutorHandle<P>,
}

struct RingFuture<P: ExecutorParams> {
    index: u32,
    ring_size: u32,
    prev: Waker,
    last: Rc<RefCell<Waker>>,
    spawned: bool,
    eh: ExecutorHandle<P>,
}

impl<P: ExecutorParams> Future for RootRingFuture<P> {
    type Item = ();
    type Error = Never;

    fn poll(&mut self, cx: &mut task::Context) -> Poll<Self::Item, Self::Error> {
        if !self.spawned {
            let w = cx.waker().clone();
            let mut eh = self.eh.clone();
            eh.spawn_local(RingFuture {
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

        if self.iteration < self.max_iterations {
            self.iteration += 1;
            self.last.borrow().wake();
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
            self.prev.wake();
            return Ok(Async::Pending);
        }

        if self.index >= self.ring_size {
            *self.last.borrow_mut() = cx.waker().clone();
            self.prev.wake();
        } else {
            let w = cx.waker().clone();
            let mut eh = self.eh.clone();
            eh.spawn_local(RingFuture {
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

struct NoopWaker;

impl Wake for NoopWaker {
    fn wake(_arc_self: &Arc<Self>) {}
}

impl NoopWaker {
    fn new() -> Waker {
        Arc::new(NoopWaker).into()
    }
}

#[cfg(test)]
mod benches {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::{NoopWaker, RootRingFuture};
    use executor::{CrossbeamUnboundedChannelImpl, Executor, ExecutorParams, TlsUnsafeImpl};

    use test::Bencher;

    macro_rules! single_suite {
        ($name:ident, $params:ty, $($fname:ident ($s:expr, $i:expr),)*) => {
            mod $name {
                use super::*;
                $(

                    #[bench]
                    fn $fname(b: &mut Bencher) {
                        do_bench::<$params>(b, $s, $i)
                    }
                )*
            }
        };
    }

    macro_rules! suite {
        ($(($name:ident,$params:ty),)*) => {
            $(
                    single_suite!(
                        $name,
                        $params,
                        chain_1_1(1, 1),
                        chain_10_1(10, 1),
                        chain_1_10(1, 10),
                        chain_1_100(1, 100),
                        chain_100_1(100, 1),
                        chain_100_100(100, 100),
                        chain_1000_1000(1000, 1000),
                        chain_1000_10000(1000, 10_000),
                        chain_10000_1000(10_000, 1000),
                    );
            )*
        };
    }

    suite!(
        (crossbeam_unbound, CrossbeamUnboundedChannelImpl),
        (tls_unsafe, TlsUnsafeImpl),
    );

    fn do_bench<P: ExecutorParams>(b: &mut Bencher, size: u32, iters: u32) {
        let w = NoopWaker::new();
        b.iter(|| {
            let mut e: Executor<P> = Executor::new();
            let eh = e.executor();
            let rf = RootRingFuture {
                ring_size: size,
                iteration: 0,
                max_iterations: iters,
                spawned: false,
                last: Rc::new(RefCell::new(w.clone())),
                eh: eh.clone(),
            };

            e.run_until(rf).unwrap();
        });
    }

}
