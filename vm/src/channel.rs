use super::value::{GosValue, RuntimeResult};
use smol::channel;
use smol::future;
use std::cell::RefCell;
use std::mem;
use std::rc::Rc;

#[derive(Clone, Debug)]
pub struct Bounded {
    // we can use Option here for send_only/recv_only channels, but would it help?
    sender: channel::Sender<GosValue>,
    receiver: channel::Receiver<GosValue>,
}

impl Bounded {
    pub fn new(cap: usize) -> Bounded {
        let (s, r) = channel::bounded(cap);
        Bounded {
            sender: s,
            receiver: r,
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.sender.len()
    }

    #[inline]
    pub fn cap(&self) -> usize {
        self.sender.capacity().unwrap()
    }

    #[inline]
    pub fn close(&self) {
        self.sender.close();
    }

    pub async fn send(&self, v: GosValue) -> RuntimeResult {
        loop {
            match self.sender.try_send(v.clone()) {
                Ok(()) => return Ok(()),
                Err(e) => match e {
                    channel::TrySendError::Full(_) => {
                        future::yield_now().await;
                    }
                    channel::TrySendError::Closed(_) => {
                        return Err("channel closed!".to_string());
                    }
                },
            }
        }
    }

    pub async fn recv(&self) -> Option<GosValue> {
        loop {
            match self.receiver.try_recv() {
                Ok(v) => return Some(v),
                Err(e) => match e {
                    channel::TryRecvError::Empty => {
                        future::yield_now().await;
                    }
                    channel::TryRecvError::Closed => return None,
                },
            }
        }
    }
}

#[derive(Clone, Debug)]
enum RendezvousState {
    Empty,
    Full(GosValue),
    Closed,
}

#[derive(Clone, Debug)]
pub struct Rendezvous {
    state: Rc<RefCell<RendezvousState>>,
}

impl Rendezvous {
    pub fn new() -> Rendezvous {
        Rendezvous {
            state: Rc::new(RefCell::new(RendezvousState::Empty)),
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        0
    }

    #[inline]
    pub fn cap(&self) -> usize {
        0
    }

    #[inline]
    pub fn close(&self) {
        *self.state.borrow_mut() = RendezvousState::Closed;
    }

    pub async fn send(&self, v: GosValue) -> RuntimeResult {
        let result = loop {
            let state_ref = self.state.borrow();
            let state: &RendezvousState = &state_ref;
            match state {
                RendezvousState::Empty => break Ok(()),
                RendezvousState::Full(_) => {
                    drop(state_ref);
                    future::yield_now().await;
                }
                RendezvousState::Closed => break Err("channel closed!".to_string()),
            }
        };
        if result.is_ok() {
            *self.state.borrow_mut() = RendezvousState::Full(v)
        }
        result
    }

    pub async fn recv(&self) -> Option<GosValue> {
        let result = loop {
            let state_ref = self.state.borrow();
            let state: &RendezvousState = &state_ref;
            match state {
                RendezvousState::Empty => {
                    drop(state_ref);
                    future::yield_now().await;
                }
                RendezvousState::Full(_) => break true,
                RendezvousState::Closed => break false,
            }
        };
        result.then(|| {
            let cur_state: &mut RendezvousState = &mut self.state.borrow_mut();
            let full = mem::replace(cur_state, RendezvousState::Empty);
            if let RendezvousState::Full(v) = full {
                v
            } else {
                unreachable!()
            }
        })
    }
}

// use trait when async_trait is ready
#[derive(Clone, Debug)]
pub enum Channel {
    Bounded(Bounded),
    Rendezvous(Rendezvous),
}

impl Channel {
    pub fn new(cap: usize) -> Channel {
        if cap == 0 {
            Channel::Rendezvous(Rendezvous::new())
        } else {
            Channel::Bounded(Bounded::new(cap))
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        match self {
            Channel::Bounded(b) => b.len(),
            Channel::Rendezvous(r) => r.len(),
        }
    }

    #[inline]
    pub fn cap(&self) -> usize {
        match self {
            Channel::Bounded(b) => b.cap(),
            Channel::Rendezvous(r) => r.cap(),
        }
    }

    #[inline]
    pub fn close(&self) {
        match self {
            Channel::Bounded(b) => b.close(),
            Channel::Rendezvous(r) => r.close(),
        }
    }

    pub async fn send(&self, v: GosValue) -> RuntimeResult {
        match self {
            Channel::Bounded(b) => b.send(v).await,
            Channel::Rendezvous(r) => r.send(v).await,
        }
    }

    pub async fn recv(&self) -> Option<GosValue> {
        match self {
            Channel::Bounded(b) => b.recv().await,
            Channel::Rendezvous(r) => r.recv().await,
        }
    }
}
