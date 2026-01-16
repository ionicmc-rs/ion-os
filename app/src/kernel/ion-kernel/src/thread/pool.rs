use alloc::{boxed::Box, collections::vec_deque::VecDeque};

use crate::thread::{self, JoinError, JoinHandle};

pub struct ThreadPool {
    max: u8,
    stack: usize,
    list: VecDeque<JoinHandle<()>>,
    queue: VecDeque<Box<dyn Fn() + Send>>,
}

impl ThreadPool {
    pub fn new(max: u8, stack: usize) -> Self {
        assert!(
            max > 1,
            "maximum size must exceed one, consider spawning threads if you only need 1"
        );

        Self {
            max,
            stack,
            list: VecDeque::with_capacity(max as usize),
            queue: VecDeque::new(),
        }
    }

    pub fn execute(&mut self, f: impl Fn() + Send + 'static) -> Result<(), usize> {
        if self.list.len() < self.max.into() {
            self.list.push_back(thread::spawn(f, self.stack));
            Ok(())
        } else {
            let left = self.queue.len();
            self.queue.push_back(Box::new(f));
            Err(left as usize)
        }
    }

    pub fn run(&mut self) -> Result<(), (usize, JoinError)> {
        let mut i = 0;
        loop {
            let next = match self.list.pop_front() {
                Some(v) => v,
                None => break Ok(()),
            };

            let next_in_queue = match self.queue.pop_front() {
                Some(ele) => thread::spawn(ele, self.stack),
                None => continue,
            };
            self.list.push_back(next_in_queue);

            // join AFTER queuing the next thread to ensure that `run` can safely be called again.
            next.join().map_err(|e| (i, e))?;
            i += 1;
        }
    }
}
