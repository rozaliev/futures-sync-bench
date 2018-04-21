use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::{Rc, Weak};
use std::sync::Arc;

use futures::executor::{Executor as FuturesExecutor, SpawnError};
use futures::prelude::*;
use futures::task::{Context, LocalMap, Wake, Waker};
use slab::Slab;

mod sync_crossbeam;
mod tls_unsafe;

pub use self::sync_crossbeam::CrossbeamUnboundedChannelImpl;
pub use self::tls_unsafe::TlsUnsafeImpl;

thread_local! {
    static WAITING: RefCell<Slab<Option<Task>>> = RefCell::new(Slab::with_capacity(4096));
}

pub trait ExecutorParams: 'static {
    type WakeReceiver: WakeReceiver;
    type WakeSender: WakeSender;

    fn pair() -> (Self::WakeSender, Self::WakeReceiver);
}

pub trait WakeReceiver {
    fn pop(&mut self) -> Option<u64>;
}

pub trait WakeSender: Sync + Send + Clone {
    fn push(&self, u64);
}

#[derive(Copy, Clone)]
struct WaitingStorage;

pub struct Executor<P: ExecutorParams> {
    run_queue: VecDeque<Task>,
    incoming: Rc<Incoming>,
    waiting: WaitingStorage,
    awakenings: P::WakeReceiver,
    aw_sender: P::WakeSender,
}

pub struct ExecutorHandle<P: ExecutorParams> {
    incoming: Weak<Incoming>,
    awakenings: P::WakeSender,
    waiting: WaitingStorage,
}

type Incoming = RefCell<VecDeque<Task>>;

struct TaskWaker<P: ExecutorParams> {
    awakenings: P::WakeSender,
    id: u64,
}

struct Task {
    id: u64,
    fut: Box<Future<Item = (), Error = Never>>,
    waker: Waker,
}

impl<P: ExecutorParams> ExecutorHandle<P> {
    fn spawn_task(&self, task: Task) -> Result<(), SpawnError> {
        let incoming = self.incoming.upgrade().ok_or(SpawnError::shutdown())?;
        incoming.borrow_mut().push_back(task);
        Ok(())
    }

    pub fn spawn_local<F>(&mut self, f: F) -> Result<(), SpawnError>
    where
        F: Future<Item = (), Error = Never> + 'static,
    {
        let id = self.waiting.get_new_task_id();
        let aw: TaskWaker<P> = TaskWaker {
            id: id,
            awakenings: self.awakenings.clone(),
        };
        self.spawn_task(Task {
            fut: Box::new(f),
            id: id,
            waker: Arc::new(aw).into(),
        })
    }
}

impl<P: ExecutorParams> Executor<P> {
    pub fn new() -> Executor<P> {
        let (s, r) = P::pair();
        Executor {
            run_queue: VecDeque::new(),
            incoming: Rc::new(RefCell::new(VecDeque::new())),
            waiting: WaitingStorage,
            awakenings: r,
            aw_sender: s,
        }
    }

    pub fn executor(&self) -> ExecutorHandle<P> {
        ExecutorHandle {
            incoming: Rc::downgrade(&self.incoming),
            awakenings: self.aw_sender.clone(),
            waiting: WaitingStorage,
        }
    }

    pub fn run_until<F>(&mut self, f: F) -> Result<(), SpawnError>
    where
        F: Future<Item = (), Error = Never> + 'static,
    {
        let mut ex = self.executor();
        ex.spawn_local(f).unwrap();
        let mut map = LocalMap::new();
        let first = self.incoming.borrow()[0].id;

        loop {
            // fill runque with new tasks
            {
                let mut inc = self.incoming.borrow_mut();
                while let Some(t) = inc.pop_front() {
                    self.run_queue.push_back(t)
                }
            }

            // check awoken tasks
            {
                while let Some(id) = self.awakenings.pop() {
                    self.run_queue.push_back(self.waiting.get_task(id));
                }
            }

            // empty run queue
            while let Some(mut task) = self.run_queue.pop_front() {
                let w = task.waker.clone();
                let mut cx = Context::new(&mut map, &w, &mut ex);
                match task.poll(&mut cx) {
                    Ok(Async::Pending) => {}
                    Ok(Async::Ready(())) => {
                        self.waiting.deregister_task(task.id);
                        if task.id == first {
                            return Ok(());
                        }
                        continue;
                    }
                    _ => unreachable!(),
                }

                self.waiting.store_task(task);
            }
        }
    }
}

impl Future for Task {
    type Item = ();
    type Error = Never;

    fn poll(&mut self, cx: &mut Context) -> Poll<(), Never> {
        self.fut.poll(cx)
    }
}

impl<P: ExecutorParams> Wake for TaskWaker<P> {
    fn wake(arc_self: &Arc<Self>) {
        arc_self.awakenings.push(arc_self.id);
    }
}

impl<P: ExecutorParams> FuturesExecutor for ExecutorHandle<P> {
    fn spawn(&mut self, f: Box<Future<Item = (), Error = Never> + Send>) -> Result<(), SpawnError> {
        let id = self.waiting.get_new_task_id();
        let aw: TaskWaker<P> = TaskWaker {
            awakenings: self.awakenings.clone(),
            id: id,
        };
        self.spawn_task(Task {
            id: id,
            waker: Arc::new(aw).into(),
            fut: f,
        })
    }

    fn status(&self) -> Result<(), SpawnError> {
        if self.incoming.upgrade().is_some() {
            Ok(())
        } else {
            Err(SpawnError::shutdown())
        }
    }
}

impl ::std::fmt::Debug for Task {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> Result<(), ::std::fmt::Error> {
        write!(f, "Task {{ id: {} }}", self.id)
    }
}

impl<P: ExecutorParams> Clone for ExecutorHandle<P> {
    fn clone(&self) -> ExecutorHandle<P> {
        ExecutorHandle {
            incoming: self.incoming.clone(),
            awakenings: self.awakenings.clone(),
            waiting: WaitingStorage,
        }
    }
}

impl WaitingStorage {
    fn get_new_task_id(&self) -> u64 {
        WAITING.with(|r| r.borrow_mut().insert(None)) as u64
    }
    fn deregister_task(&self, id: u64) {
        WAITING.with(|r| r.borrow_mut().remove(id as usize));
    }
    fn store_task(&self, task: Task) {
        let key = task.id as usize;
        WAITING.with(|r| *r.borrow_mut().get_mut(key).unwrap() = Some(task))
    }
    fn get_task(&self, id: u64) -> Task {
        WAITING.with(|r| r.borrow_mut().get_mut(id as usize).unwrap().take().unwrap())
    }
}
