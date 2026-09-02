#![no_std]

pub type ProcessId = u64;
pub const MAX_SCHEDULER_CPUS: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessState {
    Empty,
    Ready,
    Running,
    Blocked,
    Zombie,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessInfo {
    pub pid: ProcessId,
    pub parent_pid: ProcessId,
    pub state: ProcessState,
    /// Kernel-owned opaque resource, currently an address-space root.
    pub resource: u64,
    pub exit_status: u64,
}

impl ProcessInfo {
    const EMPTY: Self = Self {
        pid: 0,
        parent_pid: 0,
        state: ProcessState::Empty,
        resource: 0,
        exit_status: 0,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpawnError {
    Full,
    InvalidResource,
    PidExhausted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivateError {
    NotFound,
    NotRunnable,
    InvalidCpu,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaitResult {
    NoChild,
    Pending,
    Reaped {
        pid: ProcessId,
        resource: u64,
        exit_status: u64,
    },
}

/// Allocation-free process lifecycle and round-robin run-queue state.
/// Architecture code owns register contexts and opaque `resource` teardown.
pub struct ProcessTable<const N: usize> {
    entries: [ProcessInfo; N],
    current: [Option<usize>; MAX_SCHEDULER_CPUS],
    cursor: [usize; MAX_SCHEDULER_CPUS],
    next_pid: ProcessId,
}

impl<const N: usize> ProcessTable<N> {
    pub const fn new() -> Self {
        Self {
            entries: [ProcessInfo::EMPTY; N],
            current: [None; MAX_SCHEDULER_CPUS],
            cursor: [0; MAX_SCHEDULER_CPUS],
            next_pid: 1,
        }
    }

    pub fn spawn(&mut self, parent_pid: ProcessId, resource: u64) -> Result<ProcessId, SpawnError> {
        if resource == 0 {
            return Err(SpawnError::InvalidResource);
        }
        let index = self
            .entries
            .iter()
            .position(|entry| entry.state == ProcessState::Empty)
            .ok_or(SpawnError::Full)?;
        let pid = self.next_pid;
        self.next_pid = self
            .next_pid
            .checked_add(1)
            .filter(|next| *next != 0)
            .ok_or(SpawnError::PidExhausted)?;
        self.entries[index] = ProcessInfo {
            pid,
            parent_pid,
            state: ProcessState::Ready,
            resource,
            exit_status: 0,
        };
        Ok(pid)
    }

    pub fn activate(&mut self, pid: ProcessId) -> Result<(), ActivateError> {
        self.activate_on(0, pid)
    }

    /// Activate one ready task on a specific logical CPU. A task can be
    /// Running on at most one CPU; the table remains externally lockable so
    /// selecting distinct tasks is atomic across schedulers.
    pub fn activate_on(&mut self, cpu: usize, pid: ProcessId) -> Result<(), ActivateError> {
        if cpu >= MAX_SCHEDULER_CPUS {
            return Err(ActivateError::InvalidCpu);
        }
        let index = self.index(pid).ok_or(ActivateError::NotFound)?;
        if self.entries[index].state != ProcessState::Ready {
            return Err(ActivateError::NotRunnable);
        }
        if let Some(current) = self.current[cpu] {
            if self.entries[current].state == ProcessState::Running {
                self.entries[current].state = ProcessState::Ready;
            }
        }
        self.entries[index].state = ProcessState::Running;
        self.current[cpu] = Some(index);
        if N != 0 {
            self.cursor[cpu] = (index + 1) % N;
        }
        Ok(())
    }

    /// Preempt current process, select next ready process round-robin.
    pub fn schedule_next(&mut self) -> Option<ProcessInfo> {
        self.schedule_next_on(0)
    }

    /// Preempt this CPU's current task and select a Ready task. Entries already
    /// Running on another CPU are never eligible, preventing duplicate EL0
    /// execution of one saved context.
    pub fn schedule_next_on(&mut self, cpu: usize) -> Option<ProcessInfo> {
        self.schedule_next_where_on(cpu, |_| true)
    }

    /// Preempt this CPU's current task, then select only an eligible Ready
    /// task. Rejected entries remain Ready and unowned; useful for temporary
    /// architecture affinity while all lifecycle state remains under one lock.
    pub fn schedule_next_where_on(
        &mut self,
        cpu: usize,
        eligible: impl Fn(ProcessInfo) -> bool,
    ) -> Option<ProcessInfo> {
        if cpu >= MAX_SCHEDULER_CPUS || N == 0 {
            return None;
        }
        let start = self.current[cpu].map_or(self.cursor[cpu], |index| {
            if self.entries[index].state == ProcessState::Running {
                self.entries[index].state = ProcessState::Ready;
            }
            (index + 1) % N
        });
        self.current[cpu] = None;
        self.cursor[cpu] = start;
        for offset in 0..N {
            let index = (start + offset) % N;
            if self.entries[index].state == ProcessState::Ready && eligible(self.entries[index]) {
                self.entries[index].state = ProcessState::Running;
                self.current[cpu] = Some(index);
                self.cursor[cpu] = (index + 1) % N;
                return Some(self.entries[index]);
            }
        }
        None
    }

    pub fn block_current(&mut self) -> Option<ProcessId> {
        self.block_current_on(0)
    }

    pub fn block_current_on(&mut self, cpu: usize) -> Option<ProcessId> {
        let index = self.current.get_mut(cpu)?.take()?;
        if self.entries[index].state != ProcessState::Running {
            return None;
        }
        self.entries[index].state = ProcessState::Blocked;
        Some(self.entries[index].pid)
    }

    pub fn wake(&mut self, pid: ProcessId) -> bool {
        let Some(index) = self.index(pid) else {
            return false;
        };
        if self.entries[index].state != ProcessState::Blocked {
            return false;
        }
        self.entries[index].state = ProcessState::Ready;
        true
    }

    pub fn exit_current(&mut self, status: u64) -> Option<ProcessInfo> {
        self.exit_current_on(0, status)
    }

    pub fn exit_current_on(&mut self, cpu: usize, status: u64) -> Option<ProcessInfo> {
        let index = self.current.get_mut(cpu)?.take()?;
        if self.entries[index].state != ProcessState::Running {
            return None;
        }
        self.entries[index].state = ProcessState::Zombie;
        self.entries[index].exit_status = status;
        Some(self.entries[index])
    }

    /// Move a non-running process to Zombie for administrative teardown.
    /// The owner must still call `wait` to detach and reclaim its resource.
    pub fn terminate(&mut self, pid: ProcessId, status: u64) -> Option<ProcessInfo> {
        let index = self.index(pid)?;
        if self.current.contains(&Some(index))
            || !matches!(
                self.entries[index].state,
                ProcessState::Ready | ProcessState::Blocked
            )
        {
            return None;
        }
        self.entries[index].state = ProcessState::Zombie;
        self.entries[index].exit_status = status;
        Some(self.entries[index])
    }

    pub fn wait(&mut self, parent_pid: ProcessId, pid: ProcessId) -> WaitResult {
        let Some(index) = self.index(pid) else {
            return WaitResult::NoChild;
        };
        let entry = self.entries[index];
        if entry.parent_pid != parent_pid {
            return WaitResult::NoChild;
        }
        if entry.state != ProcessState::Zombie {
            return WaitResult::Pending;
        }
        self.entries[index] = ProcessInfo::EMPTY;
        WaitResult::Reaped {
            pid: entry.pid,
            resource: entry.resource,
            exit_status: entry.exit_status,
        }
    }

    pub fn current_pid(&self) -> Option<ProcessId> {
        self.current_pid_on(0)
    }

    pub fn current_pid_on(&self, cpu: usize) -> Option<ProcessId> {
        self.current
            .get(cpu)
            .copied()
            .flatten()
            .map(|index| self.entries[index].pid)
    }

    /// Atomically replace kernel-owned resource for running process.
    /// Used by `exec`: PID, parent, state, and exit status remain unchanged.
    pub fn replace_current_resource(&mut self, resource: u64) -> Option<u64> {
        self.replace_current_resource_on(0, resource)
    }

    pub fn replace_current_resource_on(&mut self, cpu: usize, resource: u64) -> Option<u64> {
        if resource == 0 {
            return None;
        }
        let index = self.current.get(cpu).copied().flatten()?;
        if self.entries[index].state != ProcessState::Running {
            return None;
        }
        let previous = self.entries[index].resource;
        self.entries[index].resource = resource;
        Some(previous)
    }

    pub fn get(&self, pid: ProcessId) -> Option<ProcessInfo> {
        self.index(pid).map(|index| self.entries[index])
    }

    pub fn running_cpu(&self, pid: ProcessId) -> Option<usize> {
        let index = self.index(pid)?;
        self.current.iter().position(|current| *current == Some(index))
    }

    pub fn counts(&self) -> (usize, usize, usize) {
        let live = self
            .entries
            .iter()
            .filter(|entry| !matches!(entry.state, ProcessState::Empty | ProcessState::Zombie))
            .count();
        let runnable = self
            .entries
            .iter()
            .filter(|entry| matches!(entry.state, ProcessState::Ready | ProcessState::Running))
            .count();
        let zombies = self
            .entries
            .iter()
            .filter(|entry| entry.state == ProcessState::Zombie)
            .count();
        (live, runnable, zombies)
    }

    fn index(&self, pid: ProcessId) -> Option<usize> {
        (pid != 0)
            .then(|| {
                self.entries
                    .iter()
                    .position(|entry| entry.state != ProcessState::Empty && entry.pid == pid)
            })
            .flatten()
    }
}

impl<const N: usize> Default for ProcessTable<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;

    #[test]
    fn lifecycle_requires_parent_and_preserves_resource() {
        let mut table = ProcessTable::<4>::new();
        let pid = table.spawn(7, 0x4000).unwrap();
        assert_eq!(table.wait(7, pid), WaitResult::Pending);
        assert_eq!(table.activate(pid), Ok(()));
        assert_eq!(table.current_pid(), Some(pid));
        assert_eq!(table.exit_current(42).unwrap().state, ProcessState::Zombie);
        assert_eq!(table.wait(8, pid), WaitResult::NoChild);
        assert_eq!(
            table.wait(7, pid),
            WaitResult::Reaped {
                pid,
                resource: 0x4000,
                exit_status: 42,
            }
        );
        assert_eq!(table.get(pid), None);
    }

    #[test]
    fn round_robin_skips_blocked_then_wake_restores_runnable() {
        let mut table = ProcessTable::<3>::new();
        let first = table.spawn(0, 1).unwrap();
        let second = table.spawn(0, 2).unwrap();
        table.activate(first).unwrap();
        assert_eq!(table.schedule_next().unwrap().pid, second);
        assert_eq!(table.block_current(), Some(second));
        assert_eq!(table.schedule_next().unwrap().pid, first);
        assert!(table.wake(second));
        assert_eq!(table.schedule_next().unwrap().pid, second);
    }

    #[test]
    fn blocking_preserves_cursor_and_cannot_starve_later_ready_tasks() {
        let mut table = ProcessTable::<4>::new();
        let first = table.spawn(0, 1).unwrap();
        let second = table.spawn(0, 2).unwrap();
        let third = table.spawn(0, 3).unwrap();

        assert_eq!(table.schedule_next().unwrap().pid, first);
        assert_eq!(table.block_current(), Some(first));
        assert!(table.wake(first));
        assert_eq!(table.schedule_next().unwrap().pid, second);
        assert_eq!(table.block_current(), Some(second));
        assert!(table.wake(second));
        assert_eq!(table.schedule_next().unwrap().pid, third);
    }

    #[test]
    fn full_table_reuses_reaped_slot_without_reusing_pid() {
        let mut table = ProcessTable::<1>::new();
        let first = table.spawn(0, 1).unwrap();
        assert_eq!(table.spawn(0, 2), Err(SpawnError::Full));
        table.activate(first).unwrap();
        table.exit_current(0).unwrap();
        assert!(matches!(table.wait(0, first), WaitResult::Reaped { .. }));
        let second = table.spawn(0, 2).unwrap();
        assert!(second > first);
    }

    #[test]
    fn rejects_invalid_transitions() {
        let mut table = ProcessTable::<2>::new();
        assert_eq!(table.spawn(0, 0), Err(SpawnError::InvalidResource));
        let pid = table.spawn(0, 1).unwrap();
        assert_eq!(table.activate(99), Err(ActivateError::NotFound));
        table.activate(pid).unwrap();
        assert_eq!(table.activate(pid), Err(ActivateError::NotRunnable));
        assert!(!table.wake(pid));
    }

    #[test]
    fn administrator_terminates_noncurrent_ready_or_blocked_only() {
        let mut table = ProcessTable::<3>::new();
        let shell = table.spawn(0, 1).unwrap();
        let ready = table.spawn(shell, 2).unwrap();
        let blocked = table.spawn(shell, 3).unwrap();
        table.activate(blocked).unwrap();
        assert_eq!(table.block_current(), Some(blocked));
        table.activate(shell).unwrap();
        assert!(table.terminate(shell, 143).is_none());
        assert_eq!(
            table.terminate(ready, 143).unwrap().state,
            ProcessState::Zombie
        );
        assert_eq!(
            table.terminate(blocked, 143).unwrap().state,
            ProcessState::Zombie
        );
        assert!(matches!(
            table.wait(shell, ready),
            WaitResult::Reaped {
                exit_status: 143,
                resource: 2,
                ..
            }
        ));
        assert!(matches!(
            table.wait(shell, blocked),
            WaitResult::Reaped {
                exit_status: 143,
                resource: 3,
                ..
            }
        ));
    }

    #[test]
    fn exec_replaces_only_running_resource_and_preserves_identity() {
        let mut table = ProcessTable::<2>::new();
        let pid = table.spawn(7, 0x4000).unwrap();
        assert_eq!(table.replace_current_resource(0x8000), None);
        table.activate(pid).unwrap();
        assert_eq!(table.replace_current_resource(0), None);
        assert_eq!(table.replace_current_resource(0x8000), Some(0x4000));
        assert_eq!(
            table.get(pid),
            Some(ProcessInfo {
                pid,
                parent_pid: 7,
                state: ProcessState::Running,
                resource: 0x8000,
                exit_status: 0,
            })
        );
        assert_eq!(table.current_pid(), Some(pid));
    }

    #[test]
    fn multicore_selection_never_runs_one_context_twice() {
        let mut table = ProcessTable::<8>::new();
        let tasks = [
            table.spawn(1, 0x1000).unwrap(),
            table.spawn(1, 0x1000).unwrap(),
            table.spawn(1, 0x1000).unwrap(),
            table.spawn(1, 0x1000).unwrap(),
        ];
        for cpu in 0..4 {
            let selected = table.schedule_next_on(cpu).unwrap().pid;
            assert_eq!(selected, tasks[cpu]);
            assert_eq!(table.current_pid_on(cpu), Some(tasks[cpu]));
            assert_eq!(table.running_cpu(tasks[cpu]), Some(cpu));
        }
        assert!(table.schedule_next_on(4).is_none());
        assert_eq!(table.activate_on(MAX_SCHEDULER_CPUS, tasks[0]), Err(ActivateError::InvalidCpu));

        assert_eq!(table.block_current_on(2), Some(tasks[2]));
        assert_eq!(table.current_pid_on(2), None);
        assert!(table.wake(tasks[2]));
        assert_eq!(table.schedule_next_on(2).unwrap().pid, tasks[2]);

        let exited = table.exit_current_on(3, 9).unwrap();
        assert_eq!(exited.pid, tasks[3]);
        assert_eq!(exited.state, ProcessState::Zombie);
        assert_eq!(table.current_pid_on(3), None);
        assert!(matches!(
            table.wait(1, tasks[3]),
            WaitResult::Reaped { exit_status: 9, .. }
        ));
    }

    #[test]
    fn three_independent_address_spaces_run_on_distinct_aps_with_singleton_ownership() {
        let mut table = ProcessTable::<4>::new();
        let pids = [
            table.spawn(1, 0x11_000).unwrap(),
            table.spawn(1, 0x22_000).unwrap(),
            table.spawn(1, 0x33_000).unwrap(),
        ];

        for (index, pid) in pids.iter().copied().enumerate() {
            let cpu = index + 1;
            table.activate_on(cpu, pid).unwrap();
            assert_eq!(table.current_pid_on(cpu), Some(pid));
            assert_eq!(table.running_cpu(pid), Some(cpu));
        }

        let resources = pids.map(|pid| {
            let info = table.get(pid).unwrap();
            assert_eq!(info.state, ProcessState::Running);
            assert_eq!(
                (0..4)
                    .filter(|cpu| table.current_pid_on(*cpu) == Some(pid))
                    .count(),
                1
            );
            info.resource
        });
        assert_eq!(
            pids.iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            3
        );
        assert_eq!(
            resources
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            3
        );
    }

    #[test]
    fn affinity_selection_leaves_rejected_tasks_ready_and_unowned() {
        let mut table = ProcessTable::<4>::new();
        let ui = table.spawn(0, 1).unwrap();
        let worker = table.spawn(0, 2).unwrap();
        let other = table.spawn(0, 3).unwrap();

        table.activate_on(0, ui).unwrap();
        let selected = table
            .schedule_next_where_on(1, |info| info.pid == worker)
            .unwrap();
        assert_eq!(selected.pid, worker);
        assert_eq!(table.current_pid_on(0), Some(ui));
        assert_eq!(table.current_pid_on(1), Some(worker));
        assert_eq!(table.get(other).unwrap().state, ProcessState::Ready);
        assert_eq!(table.running_cpu(other), None);

        assert!(table
            .schedule_next_where_on(2, |info| info.pid == u64::MAX)
            .is_none());
        assert_eq!(table.current_pid_on(2), None);
        assert_eq!(table.get(other).unwrap().state, ProcessState::Ready);
    }
}
