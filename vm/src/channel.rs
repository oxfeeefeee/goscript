use super::value::{GosValue, RuntimeResult};
use smol::channel;
use smol::future;
use std::cell::RefCell;
use std::mem;
use std::rc::Rc;

#[derive(Clone, Debug)]
pub enum RendezvousState {
    Empty,
    Full(GosValue),
    Closed,
}

#[derive(Clone, Debug)]
pub enum Channel {
    Bounded(channel::Sender<GosValue>, channel::Receiver<GosValue>),
    Rendezvous(Rc<RefCell<RendezvousState>>),
}

impl Channel {
    pub fn new(cap: usize) -> Channel {
        if cap == 0 {
            Channel::Rendezvous(Rc::new(RefCell::new(RendezvousState::Empty)))
        } else {
            let (s, r) = channel::bounded(cap);
            Channel::Bounded(s, r)
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        match self {
            Channel::Bounded(s, _) => s.len(),
            Channel::Rendezvous(_) => 0,
        }
    }

    #[inline]
    pub fn cap(&self) -> usize {
        match self {
            Channel::Bounded(s, _) => s.capacity().unwrap(),
            Channel::Rendezvous(_) => 0,
        }
    }

    #[inline]
    pub fn close(&self) {
        match self {
            Channel::Bounded(s, _) => {
                s.close();
            }
            Channel::Rendezvous(state) => *state.borrow_mut() = RendezvousState::Closed,
        }
    }

    pub fn try_send(&self, v: GosValue) -> Result<(), channel::TrySendError<GosValue>> {
        match self {
            Channel::Bounded(s, _) => s.try_send(v),
            Channel::Rendezvous(state) => {
                let state_ref = state.borrow();
                let s: &RendezvousState = &state_ref;
                match s {
                    RendezvousState::Empty => {
                        drop(state_ref);
                        *state.borrow_mut() = RendezvousState::Full(v);
                        Ok(())
                    }
                    RendezvousState::Full(_) => Err(channel::TrySendError::Full(v)),
                    RendezvousState::Closed => Err(channel::TrySendError::Closed(v)),
                }
            }
        }
    }

    pub fn try_recv(&self) -> Result<GosValue, channel::TryRecvError> {
        match self {
            Channel::Bounded(_, r) => r.try_recv(),
            Channel::Rendezvous(state) => {
                let state_ref = state.borrow();
                let s: &RendezvousState = &state_ref;
                match s {
                    RendezvousState::Empty => Err(channel::TryRecvError::Empty),
                    RendezvousState::Full(_) => {
                        drop(state_ref);
                        let cur_state: &mut RendezvousState = &mut state.borrow_mut();
                        let full = mem::replace(cur_state, RendezvousState::Empty);
                        if let RendezvousState::Full(v) = full {
                            Ok(v)
                        } else {
                            unreachable!()
                        }
                    }
                    RendezvousState::Closed => Err(channel::TryRecvError::Closed),
                }
            }
        }
    }

    pub async fn send(&self, v: GosValue) -> RuntimeResult {
        loop {
            match self.try_send(v.clone()) {
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
            match self.try_recv() {
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
