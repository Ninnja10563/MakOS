#![no_std]

pub const EPOLLIN: u32 = 0x001;
pub const EPOLLPRI: u32 = 0x002;
pub const EPOLLOUT: u32 = 0x004;
pub const EPOLLERR: u32 = 0x008;
pub const EPOLLHUP: u32 = 0x010;
pub const EPOLLRDNORM: u32 = 0x040;
pub const EPOLLRDBAND: u32 = 0x080;
pub const EPOLLWRNORM: u32 = 0x100;
pub const EPOLLWRBAND: u32 = 0x200;
pub const EPOLLRDHUP: u32 = 0x2000;
pub const EPOLLEXCLUSIVE: u32 = 1 << 28;
pub const EPOLLWAKEUP: u32 = 1 << 29;
pub const EPOLLONESHOT: u32 = 1 << 30;
pub const EPOLLET: u32 = 1 << 31;

/// Bounded scheduler wait key. `Any` preserves poll/epoll and signal wake
/// semantics; direct descriptor and network waits can avoid unrelated wakes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaitSource {
    Any,
    Descriptor(u64),
    Network(u64),
}

impl WaitSource {
    pub fn woken_by(self, event: Self) -> bool {
        match (self, event) {
            (Self::Any, _) | (_, Self::Any) => true,
            _ => self == event,
        }
    }
}

const HANDLE_TAG: u64 = 0x4000_0000;
const HANDLE_TAG_MASK: u64 = 0xf000_0000;
const SUPPORTED_EVENTS: u32 = EPOLLIN
    | EPOLLPRI
    | EPOLLOUT
    | EPOLLERR
    | EPOLLHUP
    | EPOLLRDNORM
    | EPOLLRDBAND
    | EPOLLWRNORM
    | EPOLLWRBAND
    | EPOLLRDHUP
    | EPOLLONESHOT
    | EPOLLET;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Event {
    pub events: u32,
    pub reserved: u32,
    pub data: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Control {
    Add,
    Delete,
    Modify,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Full,
    NotFound,
    Exists,
    Invalid,
    Permission,
}

#[derive(Clone, Copy)]
struct Watch {
    used: bool,
    target: i32,
    event: Event,
    last_ready: u32,
    disabled: bool,
}

impl Watch {
    const EMPTY: Self = Self {
        used: false,
        target: -1,
        event: Event {
            events: 0,
            reserved: 0,
            data: 0,
        },
        last_ready: 0,
        disabled: false,
    };
}

#[derive(Clone, Copy)]
struct Instance<const WATCHES: usize> {
    used: bool,
    generation: u16,
    owner: u64,
    close_on_exec: bool,
    watches: [Watch; WATCHES],
}

impl<const WATCHES: usize> Instance<WATCHES> {
    const EMPTY: Self = Self {
        used: false,
        generation: 0,
        owner: 0,
        close_on_exec: false,
        watches: [Watch::EMPTY; WATCHES],
    };
}

pub struct Table<const INSTANCES: usize, const WATCHES: usize> {
    instances: [Instance<WATCHES>; INSTANCES],
    next_generation: u16,
}

impl<const INSTANCES: usize, const WATCHES: usize> Default for Table<INSTANCES, WATCHES> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const INSTANCES: usize, const WATCHES: usize> Table<INSTANCES, WATCHES> {
    pub const fn new() -> Self {
        Self {
            instances: [Instance::EMPTY; INSTANCES],
            next_generation: 1,
        }
    }

    pub fn create(&mut self, owner: u64, close_on_exec: bool) -> Result<u64, Error> {
        if owner == 0 {
            return Err(Error::Permission);
        }
        let index = self
            .instances
            .iter()
            .position(|instance| !instance.used)
            .ok_or(Error::Full)?;
        let generation = self.next_generation.max(1);
        self.next_generation = generation.wrapping_add(1).max(1);
        self.instances[index] = Instance {
            used: true,
            generation,
            owner,
            close_on_exec,
            watches: [Watch::EMPTY; WATCHES],
        };
        Ok(encode(index, generation))
    }

    pub fn is_owned(&self, owner: u64, handle: u64) -> bool {
        self.resolve(owner, handle).is_some()
    }

    pub fn close(&mut self, owner: u64, handle: u64) -> Result<(), Error> {
        let index = self.resolve(owner, handle).ok_or(Error::NotFound)?;
        self.instances[index] = Instance::EMPTY;
        Ok(())
    }

    pub fn close_owner(&mut self, owner: u64) -> usize {
        let mut count = 0;
        for instance in &mut self.instances {
            if instance.used && instance.owner == owner {
                *instance = Instance::EMPTY;
                count += 1;
            }
        }
        count
    }

    /// POSIX epoll automatically removes a watched file description when it
    /// is closed. Remove matching watches before the numeric fd is reused.
    pub fn remove_target(&mut self, owner: u64, target: i32) -> usize {
        let mut count = 0;
        for instance in &mut self.instances {
            if !instance.used || instance.owner != owner {
                continue;
            }
            for watch in &mut instance.watches {
                if watch.used && watch.target == target {
                    *watch = Watch::EMPTY;
                    count += 1;
                }
            }
        }
        count
    }

    pub fn close_on_exec(&self, owner: u64, handle: u64) -> Result<bool, Error> {
        let index = self.resolve(owner, handle).ok_or(Error::NotFound)?;
        Ok(self.instances[index].close_on_exec)
    }

    pub fn control(
        &mut self,
        owner: u64,
        handle: u64,
        operation: Control,
        target: i32,
        event: Option<Event>,
        valid_target: impl FnOnce(i32) -> bool,
    ) -> Result<(), Error> {
        if target < 0 || self.is_owned(owner, target as u64) || !valid_target(target) {
            return Err(Error::Invalid);
        }
        let index = self.resolve(owner, handle).ok_or(Error::NotFound)?;
        let instance = &mut self.instances[index];
        let found = instance
            .watches
            .iter()
            .position(|watch| watch.used && watch.target == target);
        match operation {
            Control::Add => {
                if found.is_some() {
                    return Err(Error::Exists);
                }
                let event = validate_event(event.ok_or(Error::Invalid)?)?;
                let slot = instance
                    .watches
                    .iter()
                    .position(|watch| !watch.used)
                    .ok_or(Error::Full)?;
                instance.watches[slot] = Watch {
                    used: true,
                    target,
                    event,
                    last_ready: 0,
                    disabled: false,
                };
            }
            Control::Delete => {
                let slot = found.ok_or(Error::NotFound)?;
                instance.watches[slot] = Watch::EMPTY;
            }
            Control::Modify => {
                let slot = found.ok_or(Error::NotFound)?;
                let event = validate_event(event.ok_or(Error::Invalid)?)?;
                instance.watches[slot].event = event;
                instance.watches[slot].last_ready = 0;
                instance.watches[slot].disabled = false;
            }
        }
        Ok(())
    }

    pub fn collect(
        &mut self,
        owner: u64,
        handle: u64,
        output: &mut [Event],
        mut readiness: impl FnMut(i32, u32) -> u32,
    ) -> Result<usize, Error> {
        if output.is_empty() {
            return Err(Error::Invalid);
        }
        let index = self.resolve(owner, handle).ok_or(Error::NotFound)?;
        let mut count = 0;
        for watch in &mut self.instances[index].watches {
            if !watch.used || watch.disabled {
                continue;
            }
            let interest = watch.event.events & !(EPOLLONESHOT | EPOLLET);
            let ready =
                readiness(watch.target, interest) & (interest | EPOLLERR | EPOLLHUP | EPOLLRDHUP);
            let deliver = if watch.event.events & EPOLLET != 0 {
                ready & !watch.last_ready
            } else {
                ready
            };
            watch.last_ready = ready;
            if deliver == 0 {
                continue;
            }
            output[count] = Event {
                events: deliver,
                reserved: 0,
                data: watch.event.data,
            };
            count += 1;
            if watch.event.events & EPOLLONESHOT != 0 {
                watch.disabled = true;
            }
            if count == output.len() {
                break;
            }
        }
        Ok(count)
    }

    fn resolve(&self, owner: u64, handle: u64) -> Option<usize> {
        let (index, generation) = decode(handle)?;
        let instance = self.instances.get(index)?;
        (instance.used && instance.owner == owner && instance.generation == generation)
            .then_some(index)
    }
}

fn validate_event(mut event: Event) -> Result<Event, Error> {
    if event.events == 0
        || event.events & !SUPPORTED_EVENTS != 0
        || event.events & (EPOLLEXCLUSIVE | EPOLLWAKEUP) != 0
    {
        return Err(Error::Invalid);
    }
    event.reserved = 0;
    Ok(event)
}

fn encode(index: usize, generation: u16) -> u64 {
    HANDLE_TAG | (u64::from(generation) << 8) | (index as u64 + 1)
}

fn decode(handle: u64) -> Option<(usize, u16)> {
    if handle & HANDLE_TAG_MASK != HANDLE_TAG {
        return None;
    }
    let index = usize::try_from(handle & 0xff).ok()?.checked_sub(1)?;
    let generation = u16::try_from((handle >> 8) & 0xffff).ok()?;
    (generation != 0).then_some((index, generation))
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestTable = Table<2, 4>;

    #[test]
    fn wait_sources_target_exact_object_and_preserve_wildcards() {
        let pipe = WaitSource::Descriptor(7);
        assert!(pipe.woken_by(WaitSource::Descriptor(7)));
        assert!(!pipe.woken_by(WaitSource::Descriptor(8)));
        assert!(!pipe.woken_by(WaitSource::Network(7)));
        assert!(WaitSource::Any.woken_by(pipe));
        assert!(pipe.woken_by(WaitSource::Any));
    }

    #[test]
    fn lifecycle_and_owner_isolation() {
        let mut table = TestTable::new();
        let handle = table.create(7, true).unwrap();
        assert!(table.is_owned(7, handle));
        assert!(!table.is_owned(8, handle));
        assert_eq!(table.close_on_exec(7, handle), Ok(true));
        assert_eq!(table.close(8, handle), Err(Error::NotFound));
        assert_eq!(table.close(7, handle), Ok(()));
        assert!(!table.is_owned(7, handle));
    }

    #[test]
    fn add_modify_delete_contract() {
        let mut table = TestTable::new();
        let handle = table.create(1, false).unwrap();
        let first = Event {
            events: EPOLLIN,
            reserved: 9,
            data: 44,
        };
        assert_eq!(
            table.control(1, handle, Control::Add, 3, Some(first), |_| true),
            Ok(())
        );
        assert_eq!(
            table.control(1, handle, Control::Add, 3, Some(first), |_| true),
            Err(Error::Exists)
        );
        let changed = Event {
            events: EPOLLOUT,
            reserved: 0,
            data: 55,
        };
        assert_eq!(
            table.control(1, handle, Control::Modify, 3, Some(changed), |_| true),
            Ok(())
        );
        let mut output = [Event::default(); 1];
        assert_eq!(
            table.collect(1, handle, &mut output, |_, _| EPOLLOUT),
            Ok(1)
        );
        assert_eq!(output[0].data, 55);
        assert_eq!(
            table.control(1, handle, Control::Delete, 3, None, |_| true),
            Ok(())
        );
        assert_eq!(
            table.control(1, handle, Control::Delete, 3, None, |_| true),
            Err(Error::NotFound)
        );
    }

    #[test]
    fn level_trigger_repeats() {
        let mut table = TestTable::new();
        let handle = table.create(1, false).unwrap();
        table
            .control(
                1,
                handle,
                Control::Add,
                4,
                Some(Event {
                    events: EPOLLIN,
                    reserved: 0,
                    data: 1,
                }),
                |_| true,
            )
            .unwrap();
        let mut output = [Event::default(); 1];
        assert_eq!(table.collect(1, handle, &mut output, |_, _| EPOLLIN), Ok(1));
        assert_eq!(table.collect(1, handle, &mut output, |_, _| EPOLLIN), Ok(1));
    }

    #[test]
    fn close_target_removes_watch_before_fd_reuse() {
        let mut table = TestTable::new();
        let handle = table.create(7, false).unwrap();
        let event = Event {
            events: EPOLLIN,
            reserved: 0,
            data: 0xfeed,
        };
        table
            .control(7, handle, Control::Add, 257, Some(event), |_| true)
            .unwrap();
        assert_eq!(table.remove_target(7, 257), 1);
        assert_eq!(table.remove_target(7, 257), 0);
        let mut output = [Event::default(); 1];
        assert_eq!(
            table.collect(7, handle, &mut output, |_, _| EPOLLIN),
            Ok(0)
        );
        assert_eq!(
            table.control(7, handle, Control::Add, 257, Some(event), |_| true),
            Ok(())
        );
    }

    #[test]
    fn edge_only_reports_transition() {
        let mut table = TestTable::new();
        let handle = table.create(1, false).unwrap();
        table
            .control(
                1,
                handle,
                Control::Add,
                4,
                Some(Event {
                    events: EPOLLIN | EPOLLET,
                    reserved: 0,
                    data: 1,
                }),
                |_| true,
            )
            .unwrap();
        let mut output = [Event::default(); 1];
        assert_eq!(table.collect(1, handle, &mut output, |_, _| EPOLLIN), Ok(1));
        assert_eq!(table.collect(1, handle, &mut output, |_, _| EPOLLIN), Ok(0));
        assert_eq!(table.collect(1, handle, &mut output, |_, _| 0), Ok(0));
        assert_eq!(table.collect(1, handle, &mut output, |_, _| EPOLLIN), Ok(1));
    }

    #[test]
    fn oneshot_requires_modify_rearm() {
        let mut table = TestTable::new();
        let handle = table.create(1, false).unwrap();
        let event = Event {
            events: EPOLLIN | EPOLLONESHOT,
            reserved: 0,
            data: 9,
        };
        table
            .control(1, handle, Control::Add, 4, Some(event), |_| true)
            .unwrap();
        let mut output = [Event::default(); 1];
        assert_eq!(table.collect(1, handle, &mut output, |_, _| EPOLLIN), Ok(1));
        assert_eq!(table.collect(1, handle, &mut output, |_, _| EPOLLIN), Ok(0));
        table
            .control(1, handle, Control::Modify, 4, Some(event), |_| true)
            .unwrap();
        assert_eq!(table.collect(1, handle, &mut output, |_, _| EPOLLIN), Ok(1));
    }

    #[test]
    fn error_and_hup_are_always_reported() {
        let mut table = TestTable::new();
        let handle = table.create(1, false).unwrap();
        table
            .control(
                1,
                handle,
                Control::Add,
                4,
                Some(Event {
                    events: EPOLLIN,
                    reserved: 0,
                    data: 0,
                }),
                |_| true,
            )
            .unwrap();
        let mut output = [Event::default(); 1];
        assert_eq!(
            table.collect(1, handle, &mut output, |_, _| EPOLLERR | EPOLLHUP),
            Ok(1)
        );
        assert_eq!(output[0].events, EPOLLERR | EPOLLHUP);
    }

    #[test]
    fn capacity_and_cleanup_are_bounded() {
        let mut table = Table::<1, 1>::new();
        let handle = table.create(3, false).unwrap();
        assert_eq!(table.create(4, false), Err(Error::Full));
        table
            .control(
                3,
                handle,
                Control::Add,
                1,
                Some(Event {
                    events: EPOLLIN,
                    reserved: 0,
                    data: 0,
                }),
                |_| true,
            )
            .unwrap();
        assert_eq!(
            table.control(
                3,
                handle,
                Control::Add,
                2,
                Some(Event {
                    events: EPOLLIN,
                    reserved: 0,
                    data: 0
                }),
                |_| true
            ),
            Err(Error::Full)
        );
        assert_eq!(table.close_owner(3), 1);
    }

    #[test]
    fn rejects_bad_flags_targets_and_self_watch() {
        let mut table = TestTable::new();
        let handle = table.create(1, false).unwrap();
        assert_eq!(
            table.control(
                1,
                handle,
                Control::Add,
                -1,
                Some(Event {
                    events: EPOLLIN,
                    reserved: 0,
                    data: 0
                }),
                |_| true
            ),
            Err(Error::Invalid)
        );
        assert_eq!(
            table.control(
                1,
                handle,
                Control::Add,
                handle as i32,
                Some(Event {
                    events: EPOLLIN,
                    reserved: 0,
                    data: 0
                }),
                |_| true
            ),
            Err(Error::Invalid)
        );
        assert_eq!(
            table.control(
                1,
                handle,
                Control::Add,
                2,
                Some(Event {
                    events: EPOLLWAKEUP,
                    reserved: 0,
                    data: 0
                }),
                |_| true
            ),
            Err(Error::Invalid)
        );
    }
}
