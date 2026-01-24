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
        let s = core::str::from_utf8(&self.data[..self.length as usize]).map_err(|_| core::fmt::Error)?;
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

impl <const N: usize> HeaplessString<N> {
    pub fn new() -> Self {
        Self::default()
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
        self.data[self.length as usize..self.length as usize + ch_len].copy_from_slice(&buf[..ch_len]);
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

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::size_of;

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
}