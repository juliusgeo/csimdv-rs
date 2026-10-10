use std::ops::Index;
use std::fmt;

pub struct Record<'a> {
    data: &'a [u8],
    offsets: &'a [usize],
}

impl<'a> Record<'a> {
    pub fn new(slice: &'a [u8], offsets: &'a [usize]) -> Self {
        Record {
            data: slice,
            offsets: offsets,
        }
    }

    pub fn len(&self) -> usize {
        self.offsets.len()-1
    }

    pub fn iter(&'a mut self) -> RecordIterator<'a> {
        RecordIterator::new(self)
    }
}
impl<'a> fmt::Debug for Record<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for i in 0..self.len() {
            if i != 0 {
                write!(f, ", ")?;
            }
            write!(f, "\"{}\"", str::from_utf8(&self[i]).unwrap())?;
        }
        Ok(())
    }
}
impl<'a> Index<usize> for Record<'a> {
    type Output = [u8];
    fn index(&self, index: usize) -> &Self::Output {
        &self.data[self.offsets[index].wrapping_add(1)..self.offsets[index + 1]]
    }
}

impl<'a> PartialEq<Vec<&str>> for Record<'a> {
    fn eq(&self, other: &Vec<&str>) -> bool {
        if self.len() != other.len() {
            return false
        }
        for i in 0..self.len() {
            if str::from_utf8(&self[i]).unwrap() != other[i] {
                return false
            }
        }
        true
    }
}

pub struct RecordIterator<'a> {
    data: &'a [u8],
    separators: std::slice::Windows<'a, usize>,
}

impl<'a> RecordIterator<'a> {
    pub fn new(record: &'a Record<'a>) -> RecordIterator<'a> {
        RecordIterator {
            data: record.data,
            separators: record.offsets.windows(2),
        }
    }
}

impl<'a> Iterator for RecordIterator<'a> {
    type Item = &'a [u8];
    fn next(&mut self) -> Option<Self::Item> {
        self.separators.next().map(|w| &self.data[w[0].wrapping_add(1)..w[1]])
    }
}