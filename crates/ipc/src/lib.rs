#![no_std]

//! Allocation-free typed IPC state for kernel integration.
//!
//! This crate owns object lifetime, generation-safe handles, bounded channel
//! queues, rights attenuation, and service routing. It deliberately does not
//! copy user memory, block tasks, assign errno values, or enforce capability
//! policy beyond handle rights and same-UID/session service routing.

pub const WIRE_VERSION: u8 = 1;
pub const MESSAGE_WIRE_SIZE: usize = 64;
pub const MESSAGE_PAYLOAD_SIZE: usize = 52;
pub const CHANNEL_QUEUE_CAPACITY: usize = 16;
pub const SERVICE_REGISTRY_CAPACITY: usize = 8;
pub const SERVICE_NAME_CAPACITY: usize = 31;
pub const SERVICE_PENDING_CAPACITY: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Identity {
    pub pid: u32,
    pub uid: u32,
    pub session: u64,
}

impl Identity {
    pub const fn new(pid: u32, uid: u32, session: u64) -> Self {
        Self { pid, uid, session }
    }
}

/// Stable 64-byte userspace wire representation.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WireMessage {
    version: u8,
    length: u8,
    message_type: u16,
    sender_pid: u32,
    sender_uid: u32,
    payload: [u8; MESSAGE_PAYLOAD_SIZE],
}

impl WireMessage {
    const EMPTY: Self = Self {
        version: WIRE_VERSION,
        length: 0,
        message_type: 0,
        sender_pid: 0,
        sender_uid: 0,
        payload: [0; MESSAGE_PAYLOAD_SIZE],
    };

    pub fn new(message_type: u16, payload: &[u8]) -> Result<Self, Error> {
        if payload.len() > MESSAGE_PAYLOAD_SIZE {
            return Err(Error::MalformedMessage);
        }
        let mut message = Self {
            message_type,
            length: payload.len() as u8,
            ..Self::EMPTY
        };
        message.payload[..payload.len()].copy_from_slice(payload);
        Ok(message)
    }

    pub fn from_bytes(bytes: [u8; MESSAGE_WIRE_SIZE]) -> Result<Self, Error> {
        if bytes[0] != WIRE_VERSION || usize::from(bytes[1]) > MESSAGE_PAYLOAD_SIZE {
            return Err(Error::MalformedMessage);
        }
        let mut payload = [0; MESSAGE_PAYLOAD_SIZE];
        payload.copy_from_slice(&bytes[12..]);
        Ok(Self {
            version: bytes[0],
            length: bytes[1],
            message_type: u16::from_le_bytes([bytes[2], bytes[3]]),
            sender_pid: u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            sender_uid: u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
            payload,
        })
    }

    pub fn to_bytes(self) -> [u8; MESSAGE_WIRE_SIZE] {
        let mut bytes = [0; MESSAGE_WIRE_SIZE];
        bytes[0] = self.version;
        bytes[1] = self.length;
        bytes[2..4].copy_from_slice(&self.message_type.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.sender_pid.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.sender_uid.to_le_bytes());
        bytes[12..].copy_from_slice(&self.payload);
        bytes
    }

    pub const fn version(&self) -> u8 {
        self.version
    }

    pub const fn message_type(&self) -> u16 {
        self.message_type
    }

    pub const fn sender_pid(&self) -> u32 {
        self.sender_pid
    }

    pub const fn sender_uid(&self) -> u32 {
        self.sender_uid
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload[..usize::from(self.length)]
    }

    fn is_valid(&self) -> bool {
        self.version == WIRE_VERSION && usize::from(self.length) <= MESSAGE_PAYLOAD_SIZE
    }

    fn stamp_sender(&mut self, sender: Identity) {
        self.sender_pid = sender.pid;
        self.sender_uid = sender.uid;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rights(u8);

impl Rights {
    pub const NONE: Self = Self(0);
    pub const SEND: Self = Self(1 << 0);
    pub const RECEIVE: Self = Self(1 << 1);
    pub const TRANSFER: Self = Self(1 << 2);
    pub const ACCEPT: Self = Self(1 << 3);
    pub const CHANNEL_ALL: Self = Self(Self::SEND.0 | Self::RECEIVE.0 | Self::TRANSFER.0);

    pub const fn from_bits(bits: u8) -> Option<Self> {
        if bits & !0x0f == 0 {
            Some(Self(bits))
        } else {
            None
        }
    }

    pub const fn bits(self) -> u8 {
        self.0
    }

    pub const fn contains(self, required: Self) -> bool {
        self.0 & required.0 == required.0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Handle {
    slot: usize,
    generation: u64,
}

impl Handle {
    pub const fn from_parts(slot: usize, generation: u64) -> Self {
        Self { slot, generation }
    }

    pub const fn slot(self) -> usize {
        self.slot
    }

    pub const fn generation(self) -> u64 {
        self.generation
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Transfer {
    pub handle: Handle,
    pub rights: Rights,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReceivedMessage {
    pub message: WireMessage,
    pub transferred: Option<Handle>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CleanupStats {
    pub handles_closed: usize,
    pub services_removed: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    MalformedMessage,
    InvalidName,
    InvalidRights,
    InvalidHandle,
    StaleHandle,
    AccessDenied,
    MissingRight,
    ExcessiveRights,
    WrongObjectType,
    HandleTableFull,
    ObjectTableFull,
    QueueFull,
    QueueEmpty,
    PeerClosed,
    DuplicateService,
    ServiceTableFull,
    ServiceNotFound,
    PendingQueueFull,
    PendingQueueEmpty,
    RoutingDenied,
    RefcountOverflow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ObjectRef {
    slot: usize,
    generation: u64,
}

#[derive(Clone, Copy)]
struct QueuedTransfer {
    object: ObjectRef,
    rights: Rights,
}

#[derive(Clone, Copy)]
struct QueuedMessage {
    message: WireMessage,
    transfer: Option<QueuedTransfer>,
}

impl QueuedMessage {
    const EMPTY: Self = Self {
        message: WireMessage::EMPTY,
        transfer: None,
    };
}

#[derive(Clone, Copy)]
struct MessageQueue {
    entries: [QueuedMessage; CHANNEL_QUEUE_CAPACITY],
    head: usize,
    len: usize,
}

impl MessageQueue {
    const EMPTY: Self = Self {
        entries: [QueuedMessage::EMPTY; CHANNEL_QUEUE_CAPACITY],
        head: 0,
        len: 0,
    };

    fn push(&mut self, message: QueuedMessage) -> Result<(), Error> {
        if self.len == CHANNEL_QUEUE_CAPACITY {
            return Err(Error::QueueFull);
        }
        let tail = (self.head + self.len) % CHANNEL_QUEUE_CAPACITY;
        self.entries[tail] = message;
        self.len += 1;
        Ok(())
    }

    fn front(&self) -> Result<QueuedMessage, Error> {
        if self.len == 0 {
            Err(Error::QueueEmpty)
        } else {
            Ok(self.entries[self.head])
        }
    }

    fn pop(&mut self) -> Result<QueuedMessage, Error> {
        let message = self.front()?;
        self.entries[self.head] = QueuedMessage::EMPTY;
        self.head = (self.head + 1) % CHANNEL_QUEUE_CAPACITY;
        self.len -= 1;
        Ok(message)
    }
}

#[derive(Clone, Copy)]
struct PendingConnection {
    endpoint: ObjectRef,
    rights: Rights,
}

impl PendingConnection {
    const EMPTY: Self = Self {
        endpoint: ObjectRef {
            slot: 0,
            generation: 0,
        },
        rights: Rights::NONE,
    };
}

#[derive(Clone, Copy)]
struct PendingQueue {
    entries: [PendingConnection; SERVICE_PENDING_CAPACITY],
    head: usize,
    len: usize,
}

impl PendingQueue {
    const EMPTY: Self = Self {
        entries: [PendingConnection::EMPTY; SERVICE_PENDING_CAPACITY],
        head: 0,
        len: 0,
    };

    fn push(&mut self, connection: PendingConnection) -> Result<(), Error> {
        if self.len == SERVICE_PENDING_CAPACITY {
            return Err(Error::PendingQueueFull);
        }
        let tail = (self.head + self.len) % SERVICE_PENDING_CAPACITY;
        self.entries[tail] = connection;
        self.len += 1;
        Ok(())
    }

    fn pop(&mut self) -> Result<PendingConnection, Error> {
        if self.len == 0 {
            return Err(Error::PendingQueueEmpty);
        }
        let connection = self.entries[self.head];
        self.entries[self.head] = PendingConnection::EMPTY;
        self.head = (self.head + 1) % SERVICE_PENDING_CAPACITY;
        self.len -= 1;
        Ok(connection)
    }
}

#[derive(Clone, Copy)]
struct ChannelObject {
    peer: Option<ObjectRef>,
    queue: MessageQueue,
}

#[derive(Clone, Copy)]
enum ObjectKind {
    Free,
    Channel(ChannelObject),
    Service { registry_slot: usize },
}

#[derive(Clone, Copy)]
struct ObjectSlot {
    generation: u64,
    references: usize,
    kind: ObjectKind,
}

impl ObjectSlot {
    const EMPTY: Self = Self {
        generation: 0,
        references: 0,
        kind: ObjectKind::Free,
    };
}

#[derive(Clone, Copy)]
struct HandleSlot {
    generation: u64,
    occupied: bool,
    owner: Identity,
    object: ObjectRef,
    rights: Rights,
}

impl HandleSlot {
    const EMPTY: Self = Self {
        generation: 0,
        occupied: false,
        owner: Identity::new(0, 0, 0),
        object: ObjectRef {
            slot: 0,
            generation: 0,
        },
        rights: Rights::NONE,
    };
}

#[derive(Clone, Copy)]
struct ServiceEntry {
    occupied: bool,
    name: [u8; SERVICE_NAME_CAPACITY],
    name_len: u8,
    provider: Identity,
    pending: PendingQueue,
}

impl ServiceEntry {
    const EMPTY: Self = Self {
        occupied: false,
        name: [0; SERVICE_NAME_CAPACITY],
        name_len: 0,
        provider: Identity::new(0, 0, 0),
        pending: PendingQueue::EMPTY,
    };

    fn name_matches(&self, name: &[u8]) -> bool {
        self.occupied
            && usize::from(self.name_len) == name.len()
            && &self.name[..name.len()] == name
    }
}

/// Fixed-capacity IPC core. `H` limits process-visible handles; `O` limits
/// channel endpoint and service-listener objects.
pub struct IpcCore<const H: usize, const O: usize> {
    handles: [HandleSlot; H],
    objects: [ObjectSlot; O],
    services: [ServiceEntry; SERVICE_REGISTRY_CAPACITY],
}

impl<const H: usize, const O: usize> Default for IpcCore<H, O> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const H: usize, const O: usize> IpcCore<H, O> {
    pub const fn new() -> Self {
        Self {
            handles: [HandleSlot::EMPTY; H],
            objects: [ObjectSlot::EMPTY; O],
            services: [ServiceEntry::EMPTY; SERVICE_REGISTRY_CAPACITY],
        }
    }

    /// Create connected endpoints, optionally owned by distinct processes.
    pub fn create_channel(
        &mut self,
        left_owner: Identity,
        left_rights: Rights,
        right_owner: Identity,
        right_rights: Rights,
    ) -> Result<(Handle, Handle), Error> {
        self.validate_channel_rights(left_rights)?;
        self.validate_channel_rights(right_rights)?;
        if !self.ensure_free_objects(2) {
            return Err(Error::ObjectTableFull);
        }
        if self.free_handle_count() < 2 {
            return Err(Error::HandleTableFull);
        }

        let left = self.allocate_object(ObjectKind::Channel(ChannelObject {
            peer: None,
            queue: MessageQueue::EMPTY,
        }))?;
        let right = self.allocate_object(ObjectKind::Channel(ChannelObject {
            peer: Some(left),
            queue: MessageQueue::EMPTY,
        }))?;
        self.set_channel_peer(left, Some(right));

        let left_handle = self.install_handle(left_owner, left, left_rights, true)?;
        let right_handle = self.install_handle(right_owner, right, right_rights, true)?;
        Ok((left_handle, right_handle))
    }

    /// Register a service and return its non-transferable listener handle.
    pub fn publish(&mut self, provider: Identity, name: &[u8]) -> Result<Handle, Error> {
        self.validate_name(name)?;
        if self.find_service(name).is_some() {
            return Err(Error::DuplicateService);
        }
        let registry_slot = self
            .services
            .iter()
            .position(|entry| !entry.occupied)
            .ok_or(Error::ServiceTableFull)?;
        if !self.ensure_free_objects(1) {
            return Err(Error::ObjectTableFull);
        }
        if self.free_handle_count() == 0 {
            return Err(Error::HandleTableFull);
        }

        let object = self.allocate_object(ObjectKind::Service { registry_slot })?;
        let listener = self.install_handle(provider, object, Rights::ACCEPT, true)?;
        let mut entry = ServiceEntry {
            occupied: true,
            provider,
            ..ServiceEntry::EMPTY
        };
        entry.name_len = name.len() as u8;
        entry.name[..name.len()].copy_from_slice(name);
        self.services[registry_slot] = entry;
        Ok(listener)
    }

    /// Connect to a same-UID/session service. Server endpoint stays queued and
    /// object-retained until provider calls [`accept`](Self::accept).
    pub fn connect(&mut self, client: Identity, name: &[u8]) -> Result<Handle, Error> {
        self.validate_name(name)?;
        let registry_slot = self.find_service(name).ok_or(Error::ServiceNotFound)?;
        let provider = self.services[registry_slot].provider;
        if client.uid != provider.uid || client.session != provider.session {
            return Err(Error::RoutingDenied);
        }
        if self.services[registry_slot].pending.len == SERVICE_PENDING_CAPACITY {
            return Err(Error::PendingQueueFull);
        }
        if !self.ensure_free_objects(2) {
            return Err(Error::ObjectTableFull);
        }
        if self.free_handle_count() == 0 {
            return Err(Error::HandleTableFull);
        }

        let client_object = self.allocate_object(ObjectKind::Channel(ChannelObject {
            peer: None,
            queue: MessageQueue::EMPTY,
        }))?;
        let server_object = self.allocate_object(ObjectKind::Channel(ChannelObject {
            peer: Some(client_object),
            queue: MessageQueue::EMPTY,
        }))?;
        self.set_channel_peer(client_object, Some(server_object));
        let client_handle =
            self.install_handle(client, client_object, Rights::CHANNEL_ALL, true)?;
        self.retain_object(server_object)?;
        self.services[registry_slot]
            .pending
            .push(PendingConnection {
                endpoint: server_object,
                rights: Rights::CHANNEL_ALL,
            })?;
        Ok(client_handle)
    }

    /// Accept oldest pending connection. Queue ownership of endpoint object is
    /// adopted by returned provider handle without changing its refcount.
    pub fn accept(&mut self, provider: Identity, listener: Handle) -> Result<Handle, Error> {
        let listener_slot = self.resolve_handle(provider, listener)?;
        self.require_right(listener_slot.rights, Rights::ACCEPT)?;
        let registry_slot = match self.resolve_object(listener_slot.object)?.kind {
            ObjectKind::Service { registry_slot } => registry_slot,
            _ => return Err(Error::WrongObjectType),
        };
        if self.services[registry_slot].provider != provider {
            return Err(Error::AccessDenied);
        }
        if self.services[registry_slot].pending.len == 0 {
            return Err(Error::PendingQueueEmpty);
        }
        if self.free_handle_count() == 0 {
            return Err(Error::HandleTableFull);
        }
        let pending = self.services[registry_slot].pending.pop()?;
        self.install_handle(provider, pending.endpoint, pending.rights, false)
    }

    pub fn unpublish(&mut self, provider: Identity, listener: Handle) -> Result<(), Error> {
        let slot = self.resolve_handle(provider, listener)?;
        self.require_right(slot.rights, Rights::ACCEPT)?;
        if !matches!(
            self.resolve_object(slot.object)?.kind,
            ObjectKind::Service { .. }
        ) {
            return Err(Error::WrongObjectType);
        }
        self.close(provider, listener)
    }

    /// Enqueue typed message. Transfer duplicates one channel handle with a
    /// rights subset; queue holds an independent object reference.
    pub fn send(
        &mut self,
        sender: Identity,
        endpoint: Handle,
        mut message: WireMessage,
        transfer: Option<Transfer>,
    ) -> Result<(), Error> {
        if !message.is_valid() {
            return Err(Error::MalformedMessage);
        }
        let endpoint_slot = self.resolve_handle(sender, endpoint)?;
        self.require_right(endpoint_slot.rights, Rights::SEND)?;
        let peer = match self.resolve_object(endpoint_slot.object)?.kind {
            ObjectKind::Channel(channel) => channel.peer.ok_or(Error::PeerClosed)?,
            _ => return Err(Error::WrongObjectType),
        };
        let peer_slot = self.resolve_object(peer)?;
        match peer_slot.kind {
            ObjectKind::Channel(channel) if channel.queue.len == CHANNEL_QUEUE_CAPACITY => {
                return Err(Error::QueueFull);
            }
            ObjectKind::Channel(_) => {}
            _ => return Err(Error::PeerClosed),
        }

        let queued_transfer = if let Some(request) = transfer {
            if request.rights.is_empty() {
                return Err(Error::InvalidRights);
            }
            let source = self.resolve_handle(sender, request.handle)?;
            self.require_right(source.rights, Rights::TRANSFER)?;
            if !source.rights.contains(request.rights) {
                return Err(Error::ExcessiveRights);
            }
            if !matches!(
                self.resolve_object(source.object)?.kind,
                ObjectKind::Channel(_)
            ) {
                return Err(Error::WrongObjectType);
            }
            if !Rights::CHANNEL_ALL.contains(request.rights) {
                return Err(Error::ExcessiveRights);
            }
            self.retain_object(source.object)?;
            Some(QueuedTransfer {
                object: source.object,
                rights: request.rights,
            })
        } else {
            None
        };

        message.stamp_sender(sender);
        let queued = QueuedMessage {
            message,
            transfer: queued_transfer,
        };
        let push_result = match &mut self.objects[peer.slot].kind {
            ObjectKind::Channel(channel) => channel.queue.push(queued),
            _ => Err(Error::PeerClosed),
        };
        if let Err(error) = push_result {
            if let Some(transferred) = queued_transfer {
                self.release_object(transferred.object);
            }
            return Err(error);
        }
        Ok(())
    }

    /// Receive oldest message. If message transfers a handle, destination
    /// handle slot is reserved before dequeue so failure remains atomic.
    pub fn receive(
        &mut self,
        receiver: Identity,
        endpoint: Handle,
    ) -> Result<ReceivedMessage, Error> {
        let endpoint_slot = self.resolve_handle(receiver, endpoint)?;
        self.require_right(endpoint_slot.rights, Rights::RECEIVE)?;
        let object = endpoint_slot.object;
        let front = match self.resolve_object(object)?.kind {
            ObjectKind::Channel(channel) => channel.queue.front()?,
            _ => return Err(Error::WrongObjectType),
        };
        if front.transfer.is_some() && self.free_handle_count() == 0 {
            return Err(Error::HandleTableFull);
        }
        let queued = match &mut self.objects[object.slot].kind {
            ObjectKind::Channel(channel) => channel.queue.pop()?,
            _ => return Err(Error::WrongObjectType),
        };
        let transferred = match queued.transfer {
            Some(transfer) => {
                Some(self.install_handle(receiver, transfer.object, transfer.rights, false)?)
            }
            None => None,
        };
        Ok(ReceivedMessage {
            message: queued.message,
            transferred,
        })
    }

    pub fn close(&mut self, owner: Identity, handle: Handle) -> Result<(), Error> {
        let slot = self.resolve_handle(owner, handle)?;
        let object = slot.object;
        self.handles[handle.slot].occupied = false;
        self.handles[handle.slot].rights = Rights::NONE;
        self.release_object(object);
        self.collect_unreachable_objects();
        Ok(())
    }

    /// Close every handle owned by exact process identity. Listener closure
    /// unregisters provider services and drops all queued server endpoints.
    pub fn cleanup_identity(&mut self, owner: Identity) -> CleanupStats {
        let services_before = self.service_count();
        let mut handles_closed = 0;
        for slot in 0..H {
            if self.handles[slot].occupied && self.handles[slot].owner == owner {
                let handle = Handle {
                    slot,
                    generation: self.handles[slot].generation,
                };
                if self.close(owner, handle).is_ok() {
                    handles_closed += 1;
                }
            }
        }
        CleanupStats {
            handles_closed,
            services_removed: services_before - self.service_count(),
        }
    }

    /// Kernel reap path cleanup after credentials may already be detached.
    pub fn cleanup_pid(&mut self, pid: u32) -> CleanupStats {
        let services_before = self.service_count();
        let mut handles_closed = 0;
        for slot in 0..H {
            if self.handles[slot].occupied && self.handles[slot].owner.pid == pid {
                let owner = self.handles[slot].owner;
                let handle = Handle {
                    slot,
                    generation: self.handles[slot].generation,
                };
                if self.close(owner, handle).is_ok() {
                    handles_closed += 1;
                }
            }
        }
        CleanupStats {
            handles_closed,
            services_removed: services_before - self.service_count(),
        }
    }

    pub fn rights(&self, owner: Identity, handle: Handle) -> Result<Rights, Error> {
        Ok(self.resolve_handle(owner, handle)?.rights)
    }

    pub fn peer_is_open(&self, owner: Identity, handle: Handle) -> Result<bool, Error> {
        let slot = self.resolve_handle(owner, handle)?;
        match self.resolve_object(slot.object)?.kind {
            ObjectKind::Channel(channel) => Ok(channel.peer.is_some()),
            _ => Err(Error::WrongObjectType),
        }
    }

    pub fn queued_messages(&self, owner: Identity, handle: Handle) -> Result<usize, Error> {
        let slot = self.resolve_handle(owner, handle)?;
        match self.resolve_object(slot.object)?.kind {
            ObjectKind::Channel(channel) => Ok(channel.queue.len),
            _ => Err(Error::WrongObjectType),
        }
    }

    pub fn pending_connections(
        &self,
        provider: Identity,
        listener: Handle,
    ) -> Result<usize, Error> {
        let slot = self.resolve_handle(provider, listener)?;
        self.require_right(slot.rights, Rights::ACCEPT)?;
        let registry_slot = match self.resolve_object(slot.object)?.kind {
            ObjectKind::Service { registry_slot } => registry_slot,
            _ => return Err(Error::WrongObjectType),
        };
        Ok(self.services[registry_slot].pending.len)
    }

    pub fn service_count(&self) -> usize {
        self.services.iter().filter(|entry| entry.occupied).count()
    }

    fn validate_name(&self, name: &[u8]) -> Result<(), Error> {
        if name.is_empty()
            || name.len() > SERVICE_NAME_CAPACITY
            || name.iter().any(|byte| *byte == 0)
        {
            Err(Error::InvalidName)
        } else {
            Ok(())
        }
    }

    fn validate_channel_rights(&self, rights: Rights) -> Result<(), Error> {
        if rights.is_empty() || !Rights::CHANNEL_ALL.contains(rights) {
            Err(Error::InvalidRights)
        } else {
            Ok(())
        }
    }

    fn require_right(&self, held: Rights, required: Rights) -> Result<(), Error> {
        if held.contains(required) {
            Ok(())
        } else {
            Err(Error::MissingRight)
        }
    }

    fn find_service(&self, name: &[u8]) -> Option<usize> {
        self.services
            .iter()
            .position(|entry| entry.name_matches(name))
    }

    fn resolve_handle(&self, owner: Identity, handle: Handle) -> Result<HandleSlot, Error> {
        let Some(slot) = self.handles.get(handle.slot) else {
            return Err(Error::InvalidHandle);
        };
        if !slot.occupied || slot.generation != handle.generation {
            return Err(Error::StaleHandle);
        }
        if slot.owner != owner {
            return Err(Error::AccessDenied);
        }
        self.resolve_object(slot.object)?;
        Ok(*slot)
    }

    fn resolve_object(&self, object: ObjectRef) -> Result<ObjectSlot, Error> {
        let Some(slot) = self.objects.get(object.slot) else {
            return Err(Error::StaleHandle);
        };
        if slot.generation != object.generation || matches!(slot.kind, ObjectKind::Free) {
            Err(Error::StaleHandle)
        } else {
            Ok(*slot)
        }
    }

    fn allocate_object(&mut self, kind: ObjectKind) -> Result<ObjectRef, Error> {
        let slot = self
            .objects
            .iter()
            .position(|object| matches!(object.kind, ObjectKind::Free))
            .ok_or(Error::ObjectTableFull)?;
        let generation = next_generation(self.objects[slot].generation);
        self.objects[slot] = ObjectSlot {
            generation,
            references: 0,
            kind,
        };
        Ok(ObjectRef { slot, generation })
    }

    fn install_handle(
        &mut self,
        owner: Identity,
        object: ObjectRef,
        rights: Rights,
        retain: bool,
    ) -> Result<Handle, Error> {
        let slot = self
            .handles
            .iter()
            .position(|handle| !handle.occupied)
            .ok_or(Error::HandleTableFull)?;
        self.resolve_object(object)?;
        if retain {
            self.retain_object(object)?;
        }
        let generation = next_generation(self.handles[slot].generation);
        self.handles[slot] = HandleSlot {
            generation,
            occupied: true,
            owner,
            object,
            rights,
        };
        Ok(Handle { slot, generation })
    }

    fn retain_object(&mut self, object: ObjectRef) -> Result<(), Error> {
        self.resolve_object(object)?;
        self.objects[object.slot].references = self.objects[object.slot]
            .references
            .checked_add(1)
            .ok_or(Error::RefcountOverflow)?;
        Ok(())
    }

    fn release_object(&mut self, object: ObjectRef) {
        if self.resolve_object(object).is_err() || self.objects[object.slot].references == 0 {
            return;
        }
        self.objects[object.slot].references -= 1;
        if self.objects[object.slot].references != 0 {
            return;
        }

        let kind = self.objects[object.slot].kind;
        self.objects[object.slot].kind = ObjectKind::Free;
        match kind {
            ObjectKind::Free => {}
            ObjectKind::Service { registry_slot } => self.remove_service(registry_slot),
            ObjectKind::Channel(mut channel) => {
                if let Some(peer) = channel.peer {
                    if let Ok(peer_slot) = self.resolve_object(peer) {
                        if let ObjectKind::Channel(mut peer_channel) = peer_slot.kind {
                            if peer_channel.peer == Some(object) {
                                peer_channel.peer = None;
                                self.objects[peer.slot].kind = ObjectKind::Channel(peer_channel);
                            }
                        }
                    }
                }
                while let Ok(message) = channel.queue.pop() {
                    if let Some(transfer) = message.transfer {
                        self.release_object(transfer.object);
                    }
                }
            }
        }
    }

    fn remove_service(&mut self, registry_slot: usize) {
        if registry_slot >= SERVICE_REGISTRY_CAPACITY || !self.services[registry_slot].occupied {
            return;
        }
        let mut entry = self.services[registry_slot];
        self.services[registry_slot] = ServiceEntry::EMPTY;
        while let Ok(connection) = entry.pending.pop() {
            self.release_object(connection.endpoint);
        }
    }

    fn set_channel_peer(&mut self, object: ObjectRef, peer: Option<ObjectRef>) {
        if let ObjectKind::Channel(mut channel) = self.objects[object.slot].kind {
            channel.peer = peer;
            self.objects[object.slot].kind = ObjectKind::Channel(channel);
        }
    }

    fn free_handle_count(&self) -> usize {
        self.handles.iter().filter(|slot| !slot.occupied).count()
    }

    fn free_object_count(&self) -> usize {
        self.objects
            .iter()
            .filter(|slot| matches!(slot.kind, ObjectKind::Free))
            .count()
    }

    fn ensure_free_objects(&mut self, required: usize) -> bool {
        if self.free_object_count() < required {
            self.collect_unreachable_objects();
        }
        self.free_object_count() >= required
    }

    fn collect_unreachable_objects(&mut self) -> usize {
        let mut reachable = [false; O];
        for handle in &self.handles {
            if handle.occupied && self.object_ref_is_live(handle.object) {
                reachable[handle.object.slot] = true;
            }
        }

        loop {
            let mut changed = false;
            for slot in 0..O {
                if !reachable[slot] {
                    continue;
                }
                match self.objects[slot].kind {
                    ObjectKind::Free => {}
                    ObjectKind::Service { registry_slot } => {
                        if registry_slot >= SERVICE_REGISTRY_CAPACITY
                            || !self.services[registry_slot].occupied
                        {
                            continue;
                        }
                        let pending = self.services[registry_slot].pending;
                        for offset in 0..pending.len {
                            let index = (pending.head + offset) % SERVICE_PENDING_CAPACITY;
                            let object = pending.entries[index].endpoint;
                            if self.object_ref_is_live(object) && !reachable[object.slot] {
                                reachable[object.slot] = true;
                                changed = true;
                            }
                        }
                    }
                    ObjectKind::Channel(channel) => {
                        for offset in 0..channel.queue.len {
                            let index = (channel.queue.head + offset) % CHANNEL_QUEUE_CAPACITY;
                            if let Some(transfer) = channel.queue.entries[index].transfer {
                                if self.object_ref_is_live(transfer.object)
                                    && !reachable[transfer.object.slot]
                                {
                                    reachable[transfer.object.slot] = true;
                                    changed = true;
                                }
                            }
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }

        let mut reclaimed = 0;
        for (slot, is_reachable) in reachable.iter().copied().enumerate() {
            if !is_reachable && !matches!(self.objects[slot].kind, ObjectKind::Free) {
                if let ObjectKind::Service { registry_slot } = self.objects[slot].kind {
                    if registry_slot < SERVICE_REGISTRY_CAPACITY {
                        self.services[registry_slot] = ServiceEntry::EMPTY;
                    }
                }
                self.objects[slot].kind = ObjectKind::Free;
                self.objects[slot].references = 0;
                reclaimed += 1;
            }
        }

        for slot in 0..O {
            if let ObjectKind::Channel(mut channel) = self.objects[slot].kind {
                if channel
                    .peer
                    .is_some_and(|peer| !self.object_ref_is_live(peer))
                {
                    channel.peer = None;
                    self.objects[slot].kind = ObjectKind::Channel(channel);
                }
            }
            if !matches!(self.objects[slot].kind, ObjectKind::Free) {
                self.objects[slot].references = 0;
            }
        }
        for index in 0..H {
            let handle = self.handles[index];
            if handle.occupied && self.object_ref_is_live(handle.object) {
                self.objects[handle.object.slot].references += 1;
            }
        }
        for index in 0..SERVICE_REGISTRY_CAPACITY {
            let service = self.services[index];
            if !service.occupied {
                continue;
            }
            for offset in 0..service.pending.len {
                let entry = (service.pending.head + offset) % SERVICE_PENDING_CAPACITY;
                let object = service.pending.entries[entry].endpoint;
                if self.object_ref_is_live(object) {
                    self.objects[object.slot].references += 1;
                }
            }
        }
        for slot in 0..O {
            let ObjectKind::Channel(channel) = self.objects[slot].kind else {
                continue;
            };
            for offset in 0..channel.queue.len {
                let index = (channel.queue.head + offset) % CHANNEL_QUEUE_CAPACITY;
                if let Some(transfer) = channel.queue.entries[index].transfer {
                    if self.object_ref_is_live(transfer.object) {
                        self.objects[transfer.object.slot].references += 1;
                    }
                }
            }
        }
        reclaimed
    }

    fn object_ref_is_live(&self, object: ObjectRef) -> bool {
        self.objects.get(object.slot).is_some_and(|slot| {
            slot.generation == object.generation && !matches!(slot.kind, ObjectKind::Free)
        })
    }
}

const fn next_generation(current: u64) -> u64 {
    let next = current.wrapping_add(1);
    if next == 0 { 1 } else { next }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use core::mem::size_of;

    const PROVIDER: Identity = Identity::new(10, 1000, 7);
    const CLIENT: Identity = Identity::new(20, 1000, 7);
    const OTHER_UID: Identity = Identity::new(30, 1001, 7);
    const OTHER_SESSION: Identity = Identity::new(40, 1000, 8);

    fn message(sequence: u8) -> WireMessage {
        WireMessage::new(u16::from(sequence), &[sequence]).unwrap()
    }

    #[test]
    fn wire_shape_round_trips_and_rejects_malformed_headers() {
        assert_eq!(size_of::<WireMessage>(), MESSAGE_WIRE_SIZE);
        let mut original = WireMessage::new(0x1234, b"typed").unwrap().to_bytes();
        original[4..8].copy_from_slice(&55_u32.to_le_bytes());
        original[8..12].copy_from_slice(&77_u32.to_le_bytes());
        let decoded = WireMessage::from_bytes(original).unwrap();
        assert_eq!(decoded.message_type(), 0x1234);
        assert_eq!(decoded.payload(), b"typed");
        assert_eq!(decoded.sender_pid(), 55);
        assert_eq!(decoded.sender_uid(), 77);
        assert_eq!(decoded.to_bytes(), original);

        let mut bad_version = original;
        bad_version[0] = WIRE_VERSION + 1;
        assert_eq!(
            WireMessage::from_bytes(bad_version),
            Err(Error::MalformedMessage)
        );
        let mut bad_length = original;
        bad_length[1] = (MESSAGE_PAYLOAD_SIZE + 1) as u8;
        assert_eq!(
            WireMessage::from_bytes(bad_length),
            Err(Error::MalformedMessage)
        );
        assert_eq!(
            WireMessage::new(1, &[0; MESSAGE_PAYLOAD_SIZE + 1]),
            Err(Error::MalformedMessage)
        );
    }

    #[test]
    fn channel_is_fifo_and_full_send_is_atomic() {
        let mut core = IpcCore::<8, 8>::new();
        let (send, receive) = core
            .create_channel(PROVIDER, Rights::CHANNEL_ALL, CLIENT, Rights::CHANNEL_ALL)
            .unwrap();
        for sequence in 0..CHANNEL_QUEUE_CAPACITY as u8 {
            core.send(PROVIDER, send, message(sequence), None).unwrap();
        }
        assert_eq!(core.queued_messages(CLIENT, receive), Ok(16));
        assert_eq!(
            core.send(PROVIDER, send, message(99), None),
            Err(Error::QueueFull)
        );
        assert_eq!(core.queued_messages(CLIENT, receive), Ok(16));
        for sequence in 0..CHANNEL_QUEUE_CAPACITY as u8 {
            let received = core.receive(CLIENT, receive).unwrap();
            assert_eq!(received.message.payload(), &[sequence]);
            assert_eq!(received.message.sender_pid(), PROVIDER.pid);
            assert_eq!(received.message.sender_uid(), PROVIDER.uid);
        }
        assert_eq!(core.receive(CLIENT, receive), Err(Error::QueueEmpty));
    }

    #[test]
    fn closed_handle_never_aliases_reused_slot() {
        let mut core = IpcCore::<2, 4>::new();
        let (old, peer) = core
            .create_channel(PROVIDER, Rights::SEND, CLIENT, Rights::RECEIVE)
            .unwrap();
        core.close(PROVIDER, old).unwrap();
        core.close(CLIENT, peer).unwrap();
        let (new, _) = core
            .create_channel(PROVIDER, Rights::SEND, CLIENT, Rights::RECEIVE)
            .unwrap();
        assert_eq!(old.slot(), new.slot());
        assert_ne!(old.generation(), new.generation());
        assert_eq!(core.rights(PROVIDER, old), Err(Error::StaleHandle));
    }

    #[test]
    fn queued_transfer_survives_sender_close_and_attenuates_rights() {
        let mut core = IpcCore::<10, 8>::new();
        let (carrier_send, carrier_receive) = core
            .create_channel(PROVIDER, Rights::CHANNEL_ALL, CLIENT, Rights::CHANNEL_ALL)
            .unwrap();
        let (source, source_peer) = core
            .create_channel(PROVIDER, Rights::CHANNEL_ALL, CLIENT, Rights::CHANNEL_ALL)
            .unwrap();
        core.send(
            PROVIDER,
            carrier_send,
            WireMessage::new(8, b"handle").unwrap(),
            Some(Transfer {
                handle: source,
                rights: Rights::SEND,
            }),
        )
        .unwrap();
        core.close(PROVIDER, source).unwrap();

        let received = core.receive(CLIENT, carrier_receive).unwrap();
        let transferred = received.transferred.unwrap();
        assert_eq!(core.rights(CLIENT, transferred), Ok(Rights::SEND));
        assert_eq!(core.receive(CLIENT, transferred), Err(Error::MissingRight));
        core.send(CLIENT, transferred, message(42), None).unwrap();
        assert_eq!(
            core.receive(CLIENT, source_peer).unwrap().message.payload(),
            &[42]
        );
    }

    #[test]
    fn unreachable_queued_transfer_cycles_are_collected() {
        let mut core = IpcCore::<4, 4>::new();
        let (left, right) = core
            .create_channel(PROVIDER, Rights::CHANNEL_ALL, PROVIDER, Rights::CHANNEL_ALL)
            .unwrap();
        core.send(
            PROVIDER,
            left,
            message(1),
            Some(Transfer {
                handle: right,
                rights: Rights::SEND,
            }),
        )
        .unwrap();
        core.send(
            PROVIDER,
            right,
            message(2),
            Some(Transfer {
                handle: left,
                rights: Rights::SEND,
            }),
        )
        .unwrap();

        core.close(PROVIDER, left).unwrap();
        assert_eq!(core.free_object_count(), 3);
        core.close(PROVIDER, right).unwrap();
        assert_eq!(core.free_object_count(), 4);
        assert!(
            core.create_channel(PROVIDER, Rights::SEND, CLIENT, Rights::RECEIVE)
                .is_ok()
        );
    }

    #[test]
    fn transfer_rejects_excessive_rights_stale_handles_and_full_queue() {
        let mut core = IpcCore::<10, 8>::new();
        let (carrier_send, carrier_receive) = core
            .create_channel(PROVIDER, Rights::CHANNEL_ALL, CLIENT, Rights::CHANNEL_ALL)
            .unwrap();
        let (limited, limited_peer) = core
            .create_channel(
                PROVIDER,
                Rights::SEND.union(Rights::TRANSFER),
                CLIENT,
                Rights::RECEIVE,
            )
            .unwrap();
        assert_eq!(
            core.send(
                PROVIDER,
                carrier_send,
                message(1),
                Some(Transfer {
                    handle: limited,
                    rights: Rights::RECEIVE,
                }),
            ),
            Err(Error::ExcessiveRights)
        );
        core.close(PROVIDER, limited).unwrap();
        assert_eq!(
            core.send(
                PROVIDER,
                carrier_send,
                message(2),
                Some(Transfer {
                    handle: limited,
                    rights: Rights::SEND,
                }),
            ),
            Err(Error::StaleHandle)
        );
        for sequence in 0..CHANNEL_QUEUE_CAPACITY as u8 {
            core.send(PROVIDER, carrier_send, message(sequence), None)
                .unwrap();
        }
        assert_eq!(
            core.send(
                PROVIDER,
                carrier_send,
                message(3),
                Some(Transfer {
                    handle: limited_peer,
                    rights: Rights::RECEIVE,
                }),
            ),
            Err(Error::QueueFull)
        );
        assert_eq!(core.queued_messages(CLIENT, carrier_receive), Ok(16));
    }

    #[test]
    fn service_routes_distinct_processes_and_typed_messages() {
        let mut core = IpcCore::<16, 16>::new();
        let listener = core.publish(PROVIDER, b"org.makos.echo").unwrap();
        let client = core.connect(CLIENT, b"org.makos.echo").unwrap();
        assert_eq!(core.pending_connections(PROVIDER, listener), Ok(1));
        let server = core.accept(PROVIDER, listener).unwrap();
        assert_eq!(core.pending_connections(PROVIDER, listener), Ok(0));

        core.send(
            CLIENT,
            client,
            WireMessage::new(9, b"request").unwrap(),
            None,
        )
        .unwrap();
        let request = core.receive(PROVIDER, server).unwrap().message;
        assert_eq!(request.message_type(), 9);
        assert_eq!(request.payload(), b"request");
        assert_eq!(request.sender_pid(), CLIENT.pid);
        core.send(
            PROVIDER,
            server,
            WireMessage::new(10, b"reply").unwrap(),
            None,
        )
        .unwrap();
        assert_eq!(
            core.receive(CLIENT, client).unwrap().message.payload(),
            b"reply"
        );
    }

    #[test]
    fn service_denies_spoofing_unknown_routes_and_cross_domain_clients() {
        let mut core = IpcCore::<24, 24>::new();
        let _listener = core.publish(PROVIDER, b"org.makos.echo").unwrap();
        assert_eq!(
            core.publish(CLIENT, b"org.makos.echo"),
            Err(Error::DuplicateService)
        );
        assert_eq!(
            core.connect(CLIENT, b"org.makos.missing"),
            Err(Error::ServiceNotFound)
        );
        assert_eq!(
            core.connect(OTHER_UID, b"org.makos.echo"),
            Err(Error::RoutingDenied)
        );
        assert_eq!(
            core.connect(OTHER_SESSION, b"org.makos.echo"),
            Err(Error::RoutingDenied)
        );
        assert_eq!(core.service_count(), 1);
    }

    #[test]
    fn service_and_pending_capacities_are_exact() {
        let mut registry = IpcCore::<16, 16>::new();
        for index in 0..SERVICE_REGISTRY_CAPACITY {
            let name = [b'a' + index as u8];
            registry.publish(PROVIDER, &name).unwrap();
        }
        assert_eq!(registry.service_count(), SERVICE_REGISTRY_CAPACITY);
        assert_eq!(
            registry.publish(PROVIDER, b"overflow"),
            Err(Error::ServiceTableFull)
        );

        let mut pending = IpcCore::<32, 32>::new();
        let listener = pending.publish(PROVIDER, b"queue").unwrap();
        for index in 0..SERVICE_PENDING_CAPACITY {
            let client = Identity::new(100 + index as u32, PROVIDER.uid, PROVIDER.session);
            pending.connect(client, b"queue").unwrap();
        }
        assert_eq!(
            pending.pending_connections(PROVIDER, listener),
            Ok(SERVICE_PENDING_CAPACITY)
        );
        assert_eq!(
            pending.connect(Identity::new(999, PROVIDER.uid, PROVIDER.session), b"queue"),
            Err(Error::PendingQueueFull)
        );
    }

    #[test]
    fn provider_cleanup_unregisters_service_and_drops_pending_peers() {
        let mut core = IpcCore::<16, 16>::new();
        let _listener = core.publish(PROVIDER, b"org.makos.echo").unwrap();
        let client = core.connect(CLIENT, b"org.makos.echo").unwrap();
        assert!(core.peer_is_open(CLIENT, client).unwrap());
        let cleanup = core.cleanup_identity(PROVIDER);
        assert_eq!(cleanup.services_removed, 1);
        assert_eq!(core.service_count(), 0);
        assert_eq!(
            core.connect(CLIENT, b"org.makos.echo"),
            Err(Error::ServiceNotFound)
        );
        assert_eq!(core.peer_is_open(CLIENT, client), Ok(false));
        assert_eq!(
            core.send(CLIENT, client, message(1), None),
            Err(Error::PeerClosed)
        );
    }

    #[test]
    fn invalid_names_rights_and_foreign_handles_are_denied() {
        let mut core = IpcCore::<8, 8>::new();
        assert_eq!(core.publish(PROVIDER, b""), Err(Error::InvalidName));
        assert_eq!(
            core.publish(PROVIDER, &[b'x'; SERVICE_NAME_CAPACITY + 1]),
            Err(Error::InvalidName)
        );
        assert_eq!(
            core.publish(PROVIDER, b"bad\0name"),
            Err(Error::InvalidName)
        );
        assert_eq!(
            core.create_channel(PROVIDER, Rights::NONE, CLIENT, Rights::RECEIVE),
            Err(Error::InvalidRights)
        );
        let (handle, _) = core
            .create_channel(PROVIDER, Rights::SEND, CLIENT, Rights::RECEIVE)
            .unwrap();
        assert_eq!(core.rights(CLIENT, handle), Err(Error::AccessDenied));
    }

    #[test]
    fn receive_with_transfer_is_atomic_when_handle_table_is_full() {
        let mut core = IpcCore::<4, 8>::new();
        let (carrier_send, carrier_receive) = core
            .create_channel(PROVIDER, Rights::CHANNEL_ALL, CLIENT, Rights::CHANNEL_ALL)
            .unwrap();
        let (source, source_peer) = core
            .create_channel(PROVIDER, Rights::CHANNEL_ALL, CLIENT, Rights::CHANNEL_ALL)
            .unwrap();
        core.send(
            PROVIDER,
            carrier_send,
            message(1),
            Some(Transfer {
                handle: source,
                rights: Rights::SEND,
            }),
        )
        .unwrap();
        assert_eq!(
            core.receive(CLIENT, carrier_receive),
            Err(Error::HandleTableFull)
        );
        assert_eq!(core.queued_messages(CLIENT, carrier_receive), Ok(1));
        core.close(CLIENT, source_peer).unwrap();
        assert!(
            core.receive(CLIENT, carrier_receive)
                .unwrap()
                .transferred
                .is_some()
        );
    }
}
