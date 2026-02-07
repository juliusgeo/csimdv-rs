use crate::Index;
use std::fmt;

pub struct Record<'a> {
    data: &'a [u8],
    offsets: &'a [usize],
}

impl<'a> Record<'a> {
    pub fn new(slice: &'a [u8], offsets: &'a [usize]) -> Self {
        return Record {
            data: slice,
            offsets: offsets,
        }
    }

    pub fn len(&self) -> usize {
        return self.offsets.len()-1;
    }

    pub fn iter(&'a mut self) -> RecordIterator<'a> {
        return RecordIterator::new(self);
    }
}
impl<'a> fmt::Debug for Record<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for i in 0..self.len() {
            if i != 0 {
                write!(f, ", ")?;
            }
            write!(f, "\"{}\"", &self[i])?;
        }
        Ok(())
    }
}
impl<'a> Index<usize> for Record<'a> {
    type Output = str;
    fn index(&self, index: usize) -> &Self::Output {
        let (start, mut end) = (self.offsets[index], self.offsets[index+1]);
        if index < self.len() - 1 {
            end -= 1;
        }
        return str::from_utf8(&self.data[start..end]).unwrap();
    }
}

impl<'a> PartialEq<Vec<&str>> for Record<'a> {
    fn eq(&self, other: &Vec<&str>) -> bool {
        if self.len() != other.len() {
            return false
        }
        for i in 0..self.len() {
            if &self[i] != other[i] {
                return false
            }
        }
        return true
    }
}

pub struct RecordIterator<'a> {
    record: &'a Record<'a>,
    current_field: usize,
}

impl<'a> RecordIterator<'a> {
    pub fn new(record: &'a Record<'a>) -> RecordIterator<'a> {
        return RecordIterator {
            record: record,
            current_field: 0,
        }
    }
}

impl<'a> Iterator for RecordIterator<'a> {
    type Item = &'a [u8];
    fn next(&mut self) -> Option<Self::Item> {
        if self.record.offsets.len() == 0 || self.current_field >= self.record.offsets.len() - 1 {
            return None
        }
        let index = self.current_field;
        let (start, mut end) = (self.record.offsets[index], self.record.offsets[index + 1]);
        if index < self.record.len() - 1 {
            end -= 1;
        }
        self.current_field += 1;
        Some(&self.record.data[start..end])
    }
}