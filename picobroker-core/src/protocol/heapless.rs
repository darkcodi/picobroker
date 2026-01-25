#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HeaplessString<const N: usize> {
    length: u8,
    data: [u8; N],
}

impl<const N: usize> Default for HeaplessString<N> {
    fn default() -> Self {
        Self {
            length: 0,
            data: [0; N],
        }
    }
}

impl<const N: usize> core::fmt::Display for HeaplessString<N> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = core::str::from_utf8(&self.data[..self.length as usize])
            .map_err(|_| core::fmt::Error)?;
        write!(f, "{}", s)
    }
}

impl<const N: usize> TryFrom<&str> for HeaplessString<N> {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let bytes = value.as_bytes();
        if bytes.len() > N {
            return Err(());
        }
        let mut data = [0u8; N];
        data[..bytes.len()].copy_from_slice(bytes);
        Ok(Self {
            length: bytes.len() as u8,
            data,
        })
    }
}

impl<const N: usize> HeaplessString<N> {
    pub const fn new() -> Self {
        Self {
            length: 0,
            data: [0; N],
        }
    }

    pub const fn repeat(c: char) -> Self {
        let mut s = Self::new();
        let ch_len = c.len_utf8();
        let mut i = 0;
        while i + ch_len <= N {
            let mut buf = [0u8; 4];
            c.encode_utf8(&mut buf);
            let mut j = 0;
            while j < ch_len {
                s.data[i + j] = buf[j];
                j += 1;
            }
            s.length += ch_len as u8;
            i += ch_len;
        }
        s
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.data[..self.length as usize]).unwrap_or("")
    }

    pub fn len(&self) -> usize {
        self.length as usize
    }

    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    pub fn capacity(&self) -> usize {
        N
    }

    pub fn clear(&mut self) {
        self.length = 0;
        self.data = [0; N];
    }

    pub fn push(&mut self, ch: char) -> Result<(), ()> {
        let ch_len = ch.len_utf8();
        if (self.length as usize) + ch_len > N {
            return Err(());
        }
        let mut buf = [0u8; 4];
        ch.encode_utf8(&mut buf);
        self.data[self.length as usize..self.length as usize + ch_len]
            .copy_from_slice(&buf[..ch_len]);
        self.length += ch_len as u8;
        Ok(())
    }

    pub fn push_str(&mut self, s: &str) -> Result<(), ()> {
        let bytes = s.as_bytes();
        if (self.length as usize) + bytes.len() > N {
            return Err(());
        }
        self.data[self.length as usize..self.length as usize + bytes.len()].copy_from_slice(bytes);
        self.length += bytes.len() as u8;
        Ok(())
    }
}

impl<const N: usize> core::fmt::Write for HeaplessString<N> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        // Try to push, but truncate silently if full
        let bytes = s.as_bytes();
        let remaining = N.saturating_sub(self.length as usize);
        let to_copy = bytes.len().min(remaining);

        self.data[self.length as usize..self.length as usize + to_copy]
            .copy_from_slice(&bytes[..to_copy]);
        self.length += to_copy as u8;

        // Always return Ok to signal silent truncation
        Ok(())
    }

    fn write_char(&mut self, c: char) -> core::fmt::Result {
        // Encode char as UTF-8
        let mut buf = [0u8; 4];
        let encoded = c.encode_utf8(&mut buf);
        self.write_str(encoded)
    }
}

/// Heapless formatting macro that creates a formatted HeaplessString
///
/// Syntax: `format_heapless!(SIZE; "format string", arg1, arg2, ...)`
///
/// # Behavior
/// - Truncates silently if formatted output exceeds SIZE bytes
/// - Returns HeaplessString<SIZE> directly (no Result wrapper)
/// - SIZE must be ≤ 255 (due to u8 length field in HeaplessString)
///
/// # Example
/// ```
/// use picobroker_core::format_heapless;
/// let msg = format_heapless!(128; "Value: {}", 42);
/// assert_eq!(msg.as_str(), "Value: 42");
/// ```
#[macro_export]
macro_rules! format_heapless {
    ($size:expr; $fmt:expr $(, $($arg:expr),* $(,)?)?) => {{
        use core::fmt::Write;
        let mut buffer = $crate::HeaplessString::<$size>::new();

        // Write! will use our fmt::Write impl which truncates silently
        write!(buffer, $fmt $(, $($arg),*)?).unwrap_or_else(|_| {
            // This should never happen since our Write impl always returns Ok
            // But if it does, we still return the buffer (with partial content)
        });

        buffer
    }};
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HeaplessVec<T, const N: usize> {
    length: u16,
    data: [T; N],
}

impl<T: Default, const N: usize> Default for HeaplessVec<T, N> {
    fn default() -> Self {
        Self {
            length: 0,
            data: core::array::from_fn(|_| T::default()),
        }
    }
}

impl<T, const N: usize> HeaplessVec<T, N> {
    pub fn len(&self) -> usize {
        self.length as usize
    }

    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    pub fn capacity(&self) -> usize {
        N
    }

    pub fn clear(&mut self) {
        self.length = 0;
    }

    pub fn push(&mut self, item: T) -> Result<(), ()> {
        if (self.length as usize) >= N {
            return Err(());
        }
        self.data[self.length as usize] = item;
        self.length += 1;
        Ok(())
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        if index < self.length as usize {
            Some(&self.data[index])
        } else {
            None
        }
    }

    pub fn as_slice(&self) -> &[T] {
        &self.data[..self.length as usize]
    }

    pub fn extend_from_slice(&mut self, slice: &[T]) -> Result<(), ()>
    where
        T: Clone,
    {
        // Check if adding slice would exceed capacity
        if (self.length as usize) + slice.len() > N {
            return Err(());
        }

        // Clone each element from slice into self.data
        for (i, item) in slice.iter().enumerate() {
            self.data[self.length as usize + i] = item.clone();
        }

        // Update length
        self.length += slice.len() as u16;
        Ok(())
    }

    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&T) -> bool,
        T: Clone,
    {
        let mut write_idx = 0;
        let len = self.length as usize;

        for read_idx in 0..len {
            if f(&self.data[read_idx]) {
                // Keep this element
                if write_idx != read_idx {
                    self.data[write_idx] = self.data[read_idx].clone();
                }
                write_idx += 1;
            }
            // Else: skip element (don't increment write_idx)
        }

        self.length = write_idx as u16;
    }

    pub fn remove(&mut self, index: usize)
    where
        T: Clone,
    {
        if index >= self.length as usize {
            panic!("remove index out of bounds");
        }

        // Shift elements after index left by one position
        for i in index..self.length as usize - 1 {
            self.data[i] = self.data[i + 1].clone();
        }

        // Decrease length
        self.length -= 1;
    }
}

impl<T: Default, const N: usize> HeaplessVec<T, N> {
    pub fn new() -> Self {
        Self {
            length: 0,
            data: core::array::from_fn(|_| T::default()),
        }
    }
}

impl<const N: usize> HeaplessVec<u8, N> {
    pub const fn const_new() -> Self {
        Self {
            length: 0,
            data: [0u8; N],
        }
    }

    pub const fn repeat(n: u8) -> Self {
        let mut vec = Self::const_new();
        let mut i = 0;
        while i < N {
            vec.data[i] = n;
            vec.length += 1;
            i += 1;
        }
        vec
    }
}

impl<T, const N: usize> core::ops::Deref for HeaplessVec<T, N> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.data[..self.length as usize]
    }
}

impl<T, const N: usize> core::ops::DerefMut for HeaplessVec<T, N> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data[..self.length as usize]
    }
}

impl<'a, T, const N: usize> IntoIterator for &'a HeaplessVec<T, N> {
    type Item = &'a T;
    type IntoIter = core::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().iter()
    }
}

impl<'a, T, const N: usize> IntoIterator for &'a mut HeaplessVec<T, N> {
    type Item = &'a mut T;
    type IntoIter = core::slice::IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.data[..self.length as usize].iter_mut()
    }
}

impl<T: Default, const N: usize> IntoIterator for HeaplessVec<T, N> {
    type Item = T;
    type IntoIter = HeaplessVecIntoIterator<T, N>;

    fn into_iter(self) -> Self::IntoIter {
        HeaplessVecIntoIterator {
            vec: self,
            index: 0,
        }
    }
}

pub struct HeaplessVecIntoIterator<T: Default, const N: usize> {
    vec: HeaplessVec<T, N>,
    index: u16,
}

impl<T: Default, const N: usize> Iterator for HeaplessVecIntoIterator<T, N> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.vec.length {
            return None;
        }

        // Use core::mem::take to swap out the element with Default::default()
        let item = core::mem::take(&mut self.vec.data[self.index as usize]);
        self.index += 1;

        // If we've consumed all elements, update the length
        if self.index == self.vec.length {
            self.vec.length = 0;
        }

        Some(item)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::size_of;
    use core::fmt::Write;

    #[test]
    fn test_optimized_string_size() {
        // Check sizes of HeaplessString for lengths 0 to 32
        // Each should be N + 1 bytes (1 byte for length + N bytes for data)
        assert_eq!(size_of::<HeaplessString<0>>(), 1);
        assert_eq!(size_of::<HeaplessString<1>>(), 2);
        assert_eq!(size_of::<HeaplessString<2>>(), 3);
        assert_eq!(size_of::<HeaplessString<3>>(), 4);
        assert_eq!(size_of::<HeaplessString<4>>(), 5);
        assert_eq!(size_of::<HeaplessString<5>>(), 6);
        assert_eq!(size_of::<HeaplessString<6>>(), 7);
        assert_eq!(size_of::<HeaplessString<7>>(), 8);
        assert_eq!(size_of::<HeaplessString<8>>(), 9);
        assert_eq!(size_of::<HeaplessString<9>>(), 10);
        assert_eq!(size_of::<HeaplessString<10>>(), 11);
        assert_eq!(size_of::<HeaplessString<11>>(), 12);
        assert_eq!(size_of::<HeaplessString<12>>(), 13);
        assert_eq!(size_of::<HeaplessString<13>>(), 14);
        assert_eq!(size_of::<HeaplessString<14>>(), 15);
        assert_eq!(size_of::<HeaplessString<15>>(), 16);
        assert_eq!(size_of::<HeaplessString<16>>(), 17);
        assert_eq!(size_of::<HeaplessString<17>>(), 18);
        assert_eq!(size_of::<HeaplessString<18>>(), 19);
        assert_eq!(size_of::<HeaplessString<19>>(), 20);
        assert_eq!(size_of::<HeaplessString<20>>(), 21);
        assert_eq!(size_of::<HeaplessString<21>>(), 22);
        assert_eq!(size_of::<HeaplessString<22>>(), 23);
        assert_eq!(size_of::<HeaplessString<23>>(), 24);
        assert_eq!(size_of::<HeaplessString<24>>(), 25);
        assert_eq!(size_of::<HeaplessString<25>>(), 26);
        assert_eq!(size_of::<HeaplessString<26>>(), 27);
        assert_eq!(size_of::<HeaplessString<27>>(), 28);
        assert_eq!(size_of::<HeaplessString<28>>(), 29);
        assert_eq!(size_of::<HeaplessString<29>>(), 30);
        assert_eq!(size_of::<HeaplessString<30>>(), 31);
        assert_eq!(size_of::<HeaplessString<31>>(), 32);
        assert_eq!(size_of::<HeaplessString<32>>(), 33);
    }

    #[test]
    fn test_extend_from_slice() {
        let mut vec: HeaplessVec<u8, 5> = HeaplessVec::new();
        vec.push(1).unwrap();
        vec.push(2).unwrap();

        // Extend with empty slice
        assert!(vec.extend_from_slice(&[]).is_ok());
        assert_eq!(vec.len(), 2);

        // Extend with elements
        assert!(vec.extend_from_slice(&[3, 4]).is_ok());
        assert_eq!(vec.len(), 4);
        assert_eq!(vec.as_slice(), &[1, 2, 3, 4]);

        // Exceed capacity
        assert!(vec.extend_from_slice(&[5, 6]).is_err());
        assert_eq!(vec.len(), 4);
    }

    #[test]
    fn test_remove() {
        let mut vec: HeaplessVec<u8, 5> = HeaplessVec::new();
        vec.push(1).unwrap();
        vec.push(2).unwrap();
        vec.push(3).unwrap();
        vec.push(4).unwrap();

        // Remove from middle
        vec.remove(1);
        assert_eq!(vec.as_slice(), &[1, 3, 4]);

        // Remove from front
        vec.remove(0);
        assert_eq!(vec.as_slice(), &[3, 4]);

        // Remove from back
        vec.remove(1);
        assert_eq!(vec.as_slice(), &[3]);

        // Remove last element
        vec.remove(0);
        assert!(vec.is_empty());
    }

    #[test]
    #[should_panic(expected = "remove index out of bounds")]
    fn test_remove_out_of_bounds() {
        let mut vec: HeaplessVec<u8, 5> = HeaplessVec::new();
        vec.push(1).unwrap();
        vec.remove(5); // Should panic
    }

    #[test]
    fn test_retain() {
        let mut vec: HeaplessVec<u8, 10> = HeaplessVec::new();
        for i in 1..=5 {
            vec.push(i).unwrap();
        }

        // Keep only even numbers
        vec.retain(|x| x % 2 == 0);
        assert_eq!(vec.as_slice(), &[2, 4]);

        // Keep all
        vec.retain(|_| true);
        assert_eq!(vec.as_slice(), &[2, 4]);

        // Keep none
        vec.retain(|_| false);
        assert!(vec.is_empty());
    }

    #[test]
    fn test_vec_iteration_via_deref() {
        let mut vec: HeaplessVec<u8, 5> = HeaplessVec::new();
        vec.push(1).unwrap();
        vec.push(2).unwrap();
        vec.push(3).unwrap();

        // Test iter() from Deref
        let mut iter = vec.iter();
        assert_eq!(iter.next(), Some(&1));
        assert_eq!(iter.next(), Some(&2));
        assert_eq!(iter.next(), Some(&3));
        assert_eq!(iter.next(), None);

        // Test for loop
        let mut sum = 0;
        for item in &vec {
            sum += item;
        }
        assert_eq!(sum, 6);
    }

    // Tests for fmt::Write implementation
    #[test]
    fn test_fmt_write_basic() {
        let mut s = HeaplessString::<10>::new();
        write!(s, "hello").unwrap();
        assert_eq!(s.as_str(), "hello");
        assert_eq!(s.len(), 5);
    }

    #[test]
    fn test_fmt_write_formatting() {
        let mut s = HeaplessString::<20>::new();
        write!(s, "Hello, {}!", "world").unwrap();
        assert_eq!(s.as_str(), "Hello, world!");
    }

    #[test]
    fn test_fmt_write_truncates_on_overflow() {
        let mut s = HeaplessString::<5>::new();
        write!(s, "hello world").unwrap();
        // Should truncate to fit capacity
        assert_eq!(s.as_str(), "hello");
        assert_eq!(s.len(), 5);
    }

    #[test]
    fn test_format_heapless_basic() {
        let msg = format_heapless!(32; "Value: {}", 42);
        assert_eq!(msg.as_str(), "Value: 42");
    }

    #[test]
    fn test_format_heapless_multiple_args() {
        let a = 10;
        let b = 20;
        let msg = format_heapless!(64; "a={}, b={}, sum={}", a, b, a + b);
        assert_eq!(msg.as_str(), "a=10, b=20, sum=30");
    }

    #[test]
    fn test_format_heapless_truncates_silently() {
        let msg = format_heapless!(5; "hello world");
        // Should truncate to "hello" without panicking
        assert_eq!(msg.as_str(), "hello");
        assert_eq!(msg.len(), 5);
    }

    #[test]
    fn test_format_heapless_empty_format() {
        let msg = format_heapless!(16; "");
        assert_eq!(msg.as_str(), "");
        assert_eq!(msg.len(), 0);
    }
}

/// Single-producer-single-consumer (SPSC) bounded queue
///
/// Lock-free queue with fixed capacity. Supports any capacity (not just powers of 2).
/// Uses atomic operations for coordination between producer and consumer.
pub mod spsc {
    use core::sync::atomic::{AtomicUsize, Ordering};

    /// SPSC bounded queue with fixed capacity
    ///
    /// # Type Parameters
    ///
    /// - `T`: The type of items stored in the queue
    /// - `N`: The capacity of the queue (must be > 0)
    ///
    /// # Example
    ///
    /// ```rust
    /// use picobroker_core::protocol::heapless::spsc::Queue;
    ///
    /// let mut queue: Queue<u8, 4> = Queue::new();
    /// let (mut tx, mut rx) = queue.split();
    ///
    /// assert_eq!(tx.enqueue(1), Ok(()));
    /// assert_eq!(tx.enqueue(2), Ok(()));
    /// assert_eq!(rx.dequeue(), Some(1));
    /// assert_eq!(rx.dequeue(), Some(2));
    /// ```
    pub struct Queue<T, const N: usize> {
        /// Ring buffer storage (uninitialized, no heap allocation)
        buffer: [core::mem::MaybeUninit<T>; N],

        /// Head index - points to next item to dequeue (consumer-side)
        /// Only modified by Consumer through acquire/release semantics
        head: AtomicUsize,

        /// Tail index - points to next slot to enqueue (producer-side)
        /// Only modified by Producer through relaxed ordering
        tail: AtomicUsize,
    }

    impl<T, const N: usize> Queue<T, N> {
        /// Create a new SPSC queue with the given capacity
        ///
        /// # Panics
        ///
        /// Panics if N is 0 (empty queues are not supported)
        #[must_use]
        pub const fn new() -> Self {
            assert!(N > 0, "Queue capacity must be greater than 0");

            // SAFETY: An uninitialized array of MaybeUninit<T> is valid
            Self {
                buffer: unsafe { core::mem::MaybeUninit::uninit().assume_init() },
                head: AtomicUsize::new(0),
                tail: AtomicUsize::new(0),
            }
        }

        /// Split the queue into producer and consumer endpoints
        ///
        /// This consumes the queue and returns the producer (tx) and consumer (rx) endpoints.
        /// The producer can only enqueue items, and the consumer can only dequeue items.
        ///
        /// # Example
        ///
        /// ```rust
        /// use picobroker_core::protocol::heapless::spsc::Queue;
        ///
        /// let mut queue: Queue<u8, 4> = Queue::new();
        /// let (tx, rx) = queue.split();
        ///
        /// // tx can enqueue, rx can dequeue
        /// ```
        #[must_use]
        pub fn split(&mut self) -> (Producer<'_, T, N>, Consumer<'_, T, N>) {
            let producer = Producer {
                tail: &self.tail,
                head: &self.head,
                buffer: self.buffer.as_mut_ptr(),
            };

            let consumer = Consumer {
                head: &self.head,
                tail: &self.tail,
                buffer: self.buffer.as_ptr(),
            };

            (producer, consumer)
        }
    }

    // SAFETY: Queue is Send if T is Send because:
    // - The buffer is accessed through atomic-coordinated indices
    // - Producer and Consumer have exclusive mutable references to their respective indices
    // - The SPSC guarantee ensures no concurrent access to the same element
    unsafe impl<T: Send, const N: usize> Send for Queue<T, N> {}

    /// Producer endpoint of a SPSC queue
    ///
    /// The producer can only enqueue items into the queue.
    /// It is created by calling `Queue::split()`.
    pub struct Producer<'a, T, const N: usize> {
        /// Tail index (write position) - we own this exclusively
        tail: &'a AtomicUsize,

        /// Head index (read position) - read-only to check for available space
        head: &'a AtomicUsize,

        /// Raw pointer to the queue's buffer (safe because we use atomics for coordination)
        buffer: *mut core::mem::MaybeUninit<T>,
    }

    impl<'a, T, const N: usize> Producer<'a, T, N> {
        /// Enqueue an item into the queue
        ///
        /// # Returns
        ///
        /// - `Ok(())` - Item was successfully enqueued
        /// - `Err(item)` - Queue is full, item is returned back to caller
        ///
        /// # Example
        ///
        /// ```rust
        /// use picobroker_core::protocol::heapless::spsc::Queue;
        ///
        /// let mut queue: Queue<u8, 2> = Queue::new();
        /// let (mut tx, mut rx) = queue.split();
        ///
        /// assert_eq!(tx.enqueue(1), Ok(()));
        /// assert_eq!(tx.enqueue(2), Ok(()));
        /// assert_eq!(tx.enqueue(3), Err(3)); // Queue is full
        /// ```
        pub fn enqueue(&mut self, item: T) -> Result<(), T> {
            // Load current positions
            // Relaxed on tail: we're the exclusive writer
            let tail = self.tail.load(Ordering::Relaxed);
            // Acquire on head: we need to see consumer's latest position
            let head = self.head.load(Ordering::Acquire);

            // Check if queue is full: (tail - head) >= N
            // Using wrapping_sub to handle index overflow
            if tail.wrapping_sub(head) >= N {
                return Err(item);
            }

            // Calculate write position using modulo (supports any N)
            let idx = tail % N;

            // Write item to buffer
            // SAFETY:
            // - We checked that the queue is not full (tail != head when full)
            // - Only producer writes to this slot (after checking head)
            // - We have exclusive ownership of the item being written
            // - Raw pointer is valid for lifetime 'a and coordinated through atomic indices
            unsafe {
                self.buffer.add(idx).write(core::mem::MaybeUninit::new(item));
            }

            // Advance tail (publish the item)
            // Relaxed is sufficient: we're the exclusive writer of tail
            // Consumer will synchronize with our write using Acquire on tail
            self.tail.store(tail + 1, Ordering::Relaxed);

            Ok(())
        }
    }

    // SAFETY: Producer is Send if T is Send because:
    // - It only modifies tail (exclusive ownership in SPSC)
    // - It only reads head (atomic with proper ordering)
    // - The buffer access is coordinated through atomic indices
    unsafe impl<'a, T: Send, const N: usize> Send for Producer<'a, T, N> {}

    /// Consumer endpoint of a SPSC queue
    ///
    /// The consumer can only dequeue items from the queue.
    /// It is created by calling `Queue::split()`.
    pub struct Consumer<'a, T, const N: usize> {
        /// Head index (read position) - we own this exclusively
        head: &'a AtomicUsize,

        /// Tail index (write position) - read-only to check for available items
        tail: &'a AtomicUsize,

        /// Raw pointer to the queue's buffer (safe because we use atomics for coordination)
        buffer: *const core::mem::MaybeUninit<T>,
    }

    impl<'a, T, const N: usize> Consumer<'a, T, N> {
        /// Dequeue an item from the queue
        ///
        /// # Returns
        ///
        /// - `Some(item)` - An item was dequeued
        /// - `None` - Queue is empty
        ///
        /// # Example
        ///
        /// ```rust
        /// use picobroker_core::protocol::heapless::spsc::Queue;
        ///
        /// let mut queue: Queue<u8, 2> = Queue::new();
        /// let (mut tx, mut rx) = queue.split();
        ///
        /// assert_eq!(rx.dequeue(), None); // Empty
        /// tx.enqueue(1).unwrap();
        /// assert_eq!(rx.dequeue(), Some(1));
        /// assert_eq!(rx.dequeue(), None); // Empty again
        /// ```
        pub fn dequeue(&mut self) -> Option<T> {
            // Load current positions
            // Relaxed on head: we're the exclusive writer
            let head = self.head.load(Ordering::Relaxed);
            // Acquire on tail: we need to see producer's latest writes
            let tail = self.tail.load(Ordering::Acquire);

            // Check if queue is empty
            if head == tail {
                return None;
            }

            // Calculate read position using modulo (supports any N)
            let idx = head % N;

            // Read item from buffer
            // SAFETY:
            // - We checked that queue is not empty (head != tail)
            // - Producer initialized this slot before advancing tail
            // - Acquire ordering on tail ensures we see the producer's write
            // - Only we (consumer) read from this slot
            // - Raw pointer is valid for lifetime 'a and coordinated through atomic indices
            let item = unsafe {
                self.buffer.add(idx).read().assume_init()
            };

            // Advance head (publish that we consumed the item)
            // Release: ensures producer sees our update when they read head
            self.head.store(head + 1, Ordering::Release);

            Some(item)
        }

        /// Check if the queue is empty without dequeueing
        ///
        /// Note: This operation is lock-free but may race with concurrent
        /// enqueue/dequeue operations. The result should be considered
        /// advisory only.
        #[must_use]
        pub fn is_empty(&self) -> bool {
            let head = self.head.load(Ordering::Relaxed);
            let tail = self.tail.load(Ordering::Acquire);
            head == tail
        }
    }

    // SAFETY: Consumer is Send if T is Send because:
    // - It only modifies head (exclusive ownership in SPSC)
    // - It only reads tail (atomic with proper ordering)
    // - The buffer access is coordinated through atomic indices
    unsafe impl<'a, T: Send, const N: usize> Send for Consumer<'a, T, N> {}

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_basic_enqueue_dequeue() {
            let mut q: Queue<u8, 4> = Queue::new();
            let (mut tx, mut rx) = q.split();

            assert_eq!(tx.enqueue(1), Ok(()));
            assert_eq!(tx.enqueue(2), Ok(()));
            assert_eq!(rx.dequeue(), Some(1));
            assert_eq!(rx.dequeue(), Some(2));
            assert_eq!(rx.dequeue(), None);
        }

        #[test]
        fn test_queue_full() {
            let mut q: Queue<u8, 2> = Queue::new();
            let (mut tx, _rx) = q.split();

            assert_eq!(tx.enqueue(1), Ok(()));
            assert_eq!(tx.enqueue(2), Ok(()));
            assert_eq!(tx.enqueue(3), Err(3)); // Full
        }

        #[test]
        fn test_queue_empty() {
            let mut q: Queue<u8, 4> = Queue::new();
            let (mut tx, mut rx) = q.split();

            assert_eq!(rx.dequeue(), None);
            assert_eq!(tx.enqueue(42), Ok(()));
            assert_eq!(rx.dequeue(), Some(42));
            assert_eq!(rx.dequeue(), None);
        }

        #[test]
        fn test_wraparound() {
            let mut q: Queue<u8, 4> = Queue::new();
            let (mut tx, mut rx) = q.split();

            // Fill and drain multiple times to test wraparound
            for i in 0..8 {
                tx.enqueue(i).unwrap();
                assert_eq!(rx.dequeue(), Some(i));
            }
        }

        #[test]
        fn test_interleaved_operations() {
            let mut q: Queue<u8, 4> = Queue::new();
            let (mut tx, mut rx) = q.split();

            tx.enqueue(1).unwrap();
            assert_eq!(rx.dequeue(), Some(1));
            tx.enqueue(2).unwrap();
            assert_eq!(rx.dequeue(), Some(2));
            tx.enqueue(3).unwrap();
            tx.enqueue(4).unwrap();
            assert_eq!(rx.dequeue(), Some(3));
            assert_eq!(rx.dequeue(), Some(4));
            assert_eq!(rx.dequeue(), None);
        }

        #[test]
        fn test_non_power_of_2_capacity() {
            let mut q: Queue<u8, 3> = Queue::new();
            let (mut tx, mut rx) = q.split();

            for i in 0..3 {
                assert_eq!(tx.enqueue(i), Ok(()));
            }
            assert_eq!(tx.enqueue(3), Err(3)); // Full

            assert_eq!(rx.dequeue(), Some(0));
            assert_eq!(tx.enqueue(3), Ok(())); // Space available now
            assert_eq!(rx.dequeue(), Some(1));
            assert_eq!(rx.dequeue(), Some(2));
            assert_eq!(rx.dequeue(), Some(3));
            assert_eq!(rx.dequeue(), None);
        }

        #[test]
        fn test_is_empty() {
            let mut q: Queue<u8, 4> = Queue::new();
            let (mut tx, mut rx) = q.split();

            assert!(rx.is_empty());
            tx.enqueue(1).unwrap();
            assert!(!rx.is_empty());
            assert_eq!(rx.dequeue(), Some(1));
            assert!(rx.is_empty());
        }

        #[test]
        fn test_complex_types() {
            let mut q: Queue<&str, 4> = Queue::new();
            let (mut tx, mut rx) = q.split();

            assert_eq!(tx.enqueue("hello"), Ok(()));
            assert_eq!(tx.enqueue("world"), Ok(()));
            assert_eq!(rx.dequeue(), Some("hello"));
            assert_eq!(rx.dequeue(), Some("world"));
            assert_eq!(rx.dequeue(), None);
        }

        #[test]
        fn test_fill_then_drain() {
            let mut q: Queue<u8, 4> = Queue::new();
            let (mut tx, mut rx) = q.split();

            // Fill queue
            for i in 0..4 {
                assert_eq!(tx.enqueue(i), Ok(()));
            }

            // Should be full
            assert_eq!(tx.enqueue(4), Err(4));

            // Drain queue
            for i in 0..4 {
                assert_eq!(rx.dequeue(), Some(i));
            }

            // Should be empty
            assert_eq!(rx.dequeue(), None);
        }

        #[test]
        fn test_partial_fill_drain() {
            let mut q: Queue<u8, 10> = Queue::new();
            let (mut tx, mut rx) = q.split();

            // Partially fill
            for i in 0..5 {
                tx.enqueue(i).unwrap();
            }

            // Partially drain
            for i in 0..3 {
                assert_eq!(rx.dequeue(), Some(i));
            }

            // Add more
            for i in 5..8 {
                tx.enqueue(i).unwrap();
            }

            // Drain rest
            for i in 3..8 {
                assert_eq!(rx.dequeue(), Some(i));
            }

            assert_eq!(rx.dequeue(), None);
        }
    }
}