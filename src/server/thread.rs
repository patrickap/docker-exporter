use std::{
  sync::{mpsc, Arc, Mutex},
  thread,
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
        eprintln!("failed to execute job: {e}")
      }
    }
  }
}

impl Drop for ThreadPool {
  fn drop(&mut self) {
    drop(self.sender.take());

    for worker in &mut self.workers {
      if let Some(thread) = worker.thread.take() {
        println!("shutting down worker {}", worker.id);

        thread.join().unwrap();
      }
    }
  }
}

struct Worker {
  id: usize,
  thread: Option<thread::JoinHandle<()>>,
}

impl Worker {
  fn new(id: usize, receiver: Arc<Mutex<mpsc::Receiver<Job>>>) -> Worker {
    let thread = thread::spawn(move || loop {
      if let Ok(receiver) = receiver.lock() {
        match receiver.recv() {
          Ok(job) => job(),
          Err(_) => {
            println!("worker {id} disconnected");
            break;
          }
        }
      }
    });

    Worker {
      id,
      thread: Some(thread),
    }
  }
}

type Job = Box<dyn FnOnce() + Send + 'static>;
