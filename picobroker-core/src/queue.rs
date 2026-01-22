//! Runtime-agnostic queue abstraction for inter-handler message passing
//!
//! This module provides a lock-free, multi-producer multi-consumer (MPMC) queue
//! built on top of heapless::mpmc::Queue. It enables message passing between concurrent
//! client handlers in a no_std compatible way.

use crate::error::{Error, Result};
use crate::protocol::QoS;
use heapless::mpmc::Queue;

/// Queued message for inter-handler communication
///
/// Contains all the information needed to route a PUBLISH message to a subscriber.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedMessage<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize> {
    /// Topic name for the publish
    pub topic: heapless::String<MAX_TOPIC_NAME_LENGTH>,
    /// Payload data
    pub payload: heapless::Vec<u8, MAX_PAYLOAD_SIZE>,
    /// QoS level (always AtMostOnce for QoS 0)
    pub qos: QoS,
    /// Retain flag
    pub retain: bool,
}

/// Sender endpoint for queued messages
///
/// This type can be cloned and shared across multiple handlers to send messages
/// to a single receiver. Uses raw pointers for shared ownership of the underlying
/// heapless::mpmc::Queue.
///
/// # Safety
///
/// The sender uses raw pointers to share access to the underlying queue.
/// This is safe because:
/// - heapless::mpmc::Queue uses lock-free atomic operations
/// - The queue is owned by MessageQueue which outlives all senders/receivers
/// - Send/Sync are explicitly implemented for thread safety
pub struct MessageSender<
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_DEPTH: usize,
> {
    queue: *const Queue<QueuedMessage<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>, QUEUE_DEPTH>,
    _phantom: core::marker::PhantomData<
        QueuedMessage<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
    >,
}

// SAFETY: heapless::mpmc::Queue uses lock-free atomics, safe to share between threads/contexts
unsafe impl<
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_DEPTH: usize,
> Send for MessageSender<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_DEPTH>
{
}

unsafe impl<
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_DEPTH: usize,
> Sync for MessageSender<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_DEPTH>
{
}

impl<
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_DEPTH: usize,
> Clone for MessageSender<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_DEPTH>
{
    fn clone(&self) -> Self {
        Self {
            queue: self.queue,
            _phantom: core::marker::PhantomData,
        }
    }
}

impl<
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_DEPTH: usize,
> MessageSender<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_DEPTH>
{
    /// Try to send a message without blocking
    ///
    /// Returns Ok(()) if sent, Err(Error::QueueFull) if queue is full
    pub fn try_send(
        &self,
        message: QueuedMessage<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
    ) -> Result<()> {
        // SAFETY: queue pointer is valid as long as MessageQueue exists
        let queue = unsafe { &*self.queue };
        queue
            .enqueue(message)
            .map_err(|_| Error::QueueFull)
    }

    /// Send a message, dropping if queue is full (QoS 0 semantics)
    ///
    /// This method silently drops the message if the queue is full, which is
    /// appropriate for QoS 0 (fire and forget) messaging.
    pub fn send_or_drop(
        &self,
        message: QueuedMessage<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
    ) {
        let _ = self.try_send(message); // Drop if full
    }
}

/// Receiver endpoint for queued messages
///
/// This type receives messages from the queue. It provides a basic polling interface
/// that can be wrapped by runtime-specific async implementations (Tokio, Embassy).
pub struct MessageReceiver<
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_DEPTH: usize,
> {
    queue: *const Queue<QueuedMessage<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>, QUEUE_DEPTH>,
    _phantom: core::marker::PhantomData<
        QueuedMessage<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
    >,
}

// SAFETY: heapless::mpmc::Queue uses lock-free atomics, safe to share between threads/contexts
unsafe impl<
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_DEPTH: usize,
> Send for MessageReceiver<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_DEPTH>
{
}

unsafe impl<
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_DEPTH: usize,
> Sync for MessageReceiver<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_DEPTH>
{
}

impl<
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_DEPTH: usize,
> MessageReceiver<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_DEPTH>
{
    /// Try to receive a message without blocking
    ///
    /// Returns Some(message) if a message is available, None if queue is empty
    pub fn try_receive(
        &self,
    ) -> Option<QueuedMessage<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>> {
        // SAFETY: queue pointer is valid as long as MessageQueue exists
        let queue = unsafe { &*self.queue };
        queue.dequeue()
    }
}

/// Shared queue for message passing
///
/// This type owns the underlying heapless::mpmc::Queue and can be split into
/// sender and receiver endpoints. The queue must have a depth that is a power of 2
/// (requirement of heapless::mpmc::Queue).
///
/// # Example
///
/// ```rust
/// use picobroker_core::queue::MessageQueue;
///
/// // Create a queue with depth 32
/// let mut queue = MessageQueue::<30, 128, 32>::new();
/// let (sender, receiver) = queue.split();
///
/// // Now you can use sender to send messages
/// // and receiver to receive them
/// ```
pub struct MessageQueue<
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_DEPTH: usize,
> {
    queue: Queue<QueuedMessage<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>, QUEUE_DEPTH>,
}

impl<
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_DEPTH: usize,
> MessageQueue<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_DEPTH>
{
    /// Create a new message queue
    pub const fn new() -> Self {
        Self {
            queue: Queue::new(),
        }
    }

    /// Split into sender and receiver endpoints
    ///
    /// This method splits the queue into separate sender and receiver endpoints
    /// that can be used independently. Multiple senders can be created by cloning
    /// the sender, but only one receiver should exist.
    pub fn split(
        &mut self,
    ) -> (
        MessageSender<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_DEPTH>,
        MessageReceiver<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_DEPTH>,
    ) {
        let queue_ptr = &self.queue as *const _;
        (
            MessageSender {
                queue: queue_ptr,
                _phantom: core::marker::PhantomData,
            },
            MessageReceiver {
                queue: queue_ptr,
                _phantom: core::marker::PhantomData,
            },
        )
    }
}

impl<
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_DEPTH: usize,
> Default for MessageQueue<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_DEPTH>
{
    fn default() -> Self {
        Self::new()
    }
}
