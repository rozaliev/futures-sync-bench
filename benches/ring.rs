#[macro_use]
extern crate criterion;
extern crate futures_sync_bench;

use std::cell::RefCell;
use std::rc::Rc;

use criterion::{Bencher, Criterion, Fun};

use futures_sync_bench::cases::ring::{InitFuture, NoopWaker, RootRingFuture};
use futures_sync_bench::executor::{CrossbeamUnboundedChannelImpl, Executor, ExecutorParams,
                                   TlsUnsafeImpl};

fn do_bench<P: ExecutorParams>(b: &mut Bencher, size: u32) {
    let mut e: Executor<P> = Executor::new();
    let eh = e.executor();

    let last_waker = Rc::new(RefCell::new(NoopWaker::new()));
    let root_waker = Rc::new(RefCell::new(NoopWaker::new()));

    let init = InitFuture {
        ring_size: size,
        spawned: false,
        last: last_waker.clone(),
        eh: eh.clone(),
        root: root_waker.clone(),
    };
    e.run_until(init).unwrap();

    b.iter(|| {
        let rf = RootRingFuture {
            last: last_waker.borrow().clone(),
            root: root_waker.clone(),
            fired: false,
        };

        e.run_until(rf).unwrap();
    });
}

fn chain(c: &mut Criterion) {
    let inputs = vec![1, 10, 100, 1000, 10_000];

    c.bench_function_over_inputs(
        "chain tls unsafe",
        |b, s| do_bench::<TlsUnsafeImpl>(b, *s),
        inputs.clone(),
    );
    c.bench_function_over_inputs(
        "chain crossbeam unbound",
        |b, s| do_bench::<CrossbeamUnboundedChannelImpl>(b, *s),
        inputs.clone(),
    );
}

fn chain_compare(c: &mut Criterion) {
    let tls = Fun::new("tls unsafe", |b, s| do_bench::<TlsUnsafeImpl>(b, *s));
    let crossbeam = Fun::new("crossbeam", |b, s| {
        do_bench::<CrossbeamUnboundedChannelImpl>(b, *s)
    });

    c.bench_functions("compare chain 10k", vec![tls, crossbeam], 10_000);
}

criterion_group!(taskchain, chain_compare);
criterion_main!(taskchain);
