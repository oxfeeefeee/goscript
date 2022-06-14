// Copyright 2022 The Goscript Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

use super::instruction::*;
use super::value::*;
use futures_lite::future;
use rand::prelude::*;
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
    Bounded(
        async_channel::Sender<GosValue>,
        async_channel::Receiver<GosValue>,
    ),
    Rendezvous(Rc<RefCell<RendezvousState>>),
}

impl Channel {
    pub fn new(cap: usize) -> Channel {
        if cap == 0 {
            Channel::Rendezvous(Rc::new(RefCell::new(RendezvousState::Empty)))
        } else {
            let (s, r) = async_channel::bounded(cap);
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

    pub fn try_send(&self, v: GosValue) -> Result<(), async_channel::TrySendError<GosValue>> {
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
                    RendezvousState::Full(_) => Err(async_channel::TrySendError::Full(v)),
                    RendezvousState::Closed => Err(async_channel::TrySendError::Closed(v)),
                }
            }
        }
    }

    pub fn try_recv(&self) -> Result<GosValue, async_channel::TryRecvError> {
        match self {
            Channel::Bounded(_, r) => r.try_recv(),
            Channel::Rendezvous(state) => {
                let state_ref = state.borrow();
                let s: &RendezvousState = &state_ref;
                match s {
                    RendezvousState::Empty => Err(async_channel::TryRecvError::Empty),
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
                    RendezvousState::Closed => Err(async_channel::TryRecvError::Closed),
                }
            }
        }
    }

    pub async fn send(&self, v: &GosValue) -> RuntimeResult<()> {
        loop {
            match self.try_send(v.clone()) {
                Ok(()) => return Ok(()),
                Err(e) => match e {
                    async_channel::TrySendError::Full(_) => {
                        future::yield_now().await;
                    }
                    async_channel::TrySendError::Closed(_) => {
                        return Err("channel closed!".to_owned());
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
                    async_channel::TryRecvError::Empty => {
                        future::yield_now().await;
                    }
                    async_channel::TryRecvError::Closed => return None,
                },
            }
        }
    }
}

pub enum SelectCommType {
    Send(GosValue),
    Recv(ValueType, OpIndex),
}

pub struct SelectComm {
    pub typ: SelectCommType,
    pub chan: GosValue,
    pub offset: OpIndex,
}

pub struct Selector {
    pub comms: Vec<SelectComm>,
    pub default_offset: Option<OpIndex>,
}

impl Selector {
    pub fn new(comms: Vec<SelectComm>, default_offset: Option<OpIndex>) -> Selector {
        Selector {
            comms,
            default_offset,
        }
    }

    pub async fn select(&self) -> RuntimeResult<(usize, Option<GosValue>)> {
        let count = self.comms.len();
        let mut rng = rand::thread_rng();
        loop {
            for (i, entry) in self
                .comms
                .iter()
                .enumerate()
                .choose_multiple(&mut rng, count)
            {
                match &entry.typ {
                    SelectCommType::Send(val) => {
                        match entry.chan.as_some_channel()?.chan.try_send(val.clone()) {
                            Ok(_) => return Ok((i, None)),
                            Err(e) => match e {
                                async_channel::TrySendError::Full(_) => {}
                                async_channel::TrySendError::Closed(_) => {
                                    return Err("channel closed!".to_owned());
                                }
                            },
                        }
                    }
                    SelectCommType::Recv(_, _) => {
                        match entry.chan.as_some_channel()?.chan.try_recv() {
                            Ok(v) => return Ok((i, Some(v))),
                            Err(e) => match e {
                                async_channel::TryRecvError::Empty => {}
                                async_channel::TryRecvError::Closed => return Ok((i, None)),
                            },
                        }
                    }
                }
            }

            if let Some(_) = self.default_offset {
                return Ok((self.comms.len(), None));
            }
            future::yield_now().await;
        }
    }
}
