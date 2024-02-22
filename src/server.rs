use std::{
  sync::{mpsc, Arc, Mutex},
  thread::{self},
};

pub struct ThreadPool {
  sender: Option<mpsc::Sender<Job>>,
  workers: Vec<Worker>,
}

impl ThreadPool {
  pub fn new(size: usize) -> ThreadPool {
    let (sender, receiver) = mpsc::channel();
    let receiver = Arc::new(Mutex::new(receiver));
    let mut workers = Vec::with_capacity(size);

    for id in 0..size {
      workers.push(Worker::new(id, Arc::clone(&receiver)));
    }

    ThreadPool {
      sender: Some(sender),
      workers,
    }
  }

  pub fn execute<F: FnOnce() + Send + 'static>(&self, f: F) {
    let job = Box::new(f);
    if let Some(sender) = &self.sender {
      if let Err(e) = sender.send(job) {
        // TODO: error
      }
    }
  }
}

impl Drop for ThreadPool {
  fn drop(&mut self) {
    drop(self.sender.take());

    for worker in &mut self.workers {
      if let Some(thread) = worker.thread.take() {
        thread.join().unwrap();
      }
    }
  }
}

pub struct Worker {
  id: usize,
  thread: Option<thread::JoinHandle<()>>,
}

impl Worker {
  pub fn new(id: usize, receiver: Arc<Mutex<mpsc::Receiver<Job>>>) -> Worker {
    let thread = thread::spawn(move || loop {
      if let Ok(receiver) = receiver.lock() {
        match receiver.recv() {
          Ok(job) => job(),
          Err(_) => break,
        }
      }
    });

    Worker {
      id,
      thread: Some(thread),
    }
  }
}

pub type Job = Box<dyn FnOnce() + Send + 'static>;
