use std::marker::PhantomData;

use serde::de::DeserializeOwned;
use crate::aligned_buffer::AlignedBuffer;
use crate::de::{DeError, Headers, deserialize_record};
use crate::Dialect;
use crate::parser::Parser;
use crate::record::Record;

// Reads bytes directly, with no escaping/no copying/no borrowing
pub struct ByteReader {
    parser: Parser,
    headers: Option<Headers>,
}

impl ByteReader {
    /// A reader with no header row: every line is data.
    pub fn new(parser: Parser) -> Self {
        ByteReader { parser, headers: None }
    }

    pub fn with_headers(mut parser: Parser) -> Self {
        let headers = parser.read_line().map(|record| Headers::from_record(&record));
        ByteReader { parser, headers }
    }

    pub fn headers(&self) -> Option<&Headers> {
        self.headers.as_ref()
    }

    pub fn next_record(&mut self) -> Option<Record<'_>> {
        self.parser.read_line()
    }

    pub fn next_record_with_headers(&mut self) -> Option<(Record<'_>, Option<&Headers>)> {
        // Disjoint field borrows: `headers` shared, `parser` mutable.
        let headers = self.headers.as_ref();
        let record = self.parser.read_line()?;
        Some((record, headers))
    }
}

/// Reads each row into `D`. If fields are set to be deserialized as `String`, they will be escaped.
/// Deserializing to &str is not allowed. Will automatically check for headers.
pub struct SerdeReader<D> {
    inner: ByteReader,
    _marker: PhantomData<D>,
    dialect: Dialect,
}

impl<D> SerdeReader<D> {
    pub fn new(dialect: Dialect, file: &std::fs::File) -> Self {
        let bufreader = AlignedBuffer::new(&file).unwrap();
        let inner = ByteReader::with_headers(Parser::new(dialect, bufreader));
        SerdeReader { inner, _marker: PhantomData, dialect: dialect }
    }

    pub fn headers(&self) -> Option<&Headers> {
        self.inner.headers()
    }

    pub fn into_inner(self) -> ByteReader {
        self.inner
    }
}

impl<D: DeserializeOwned> Iterator for SerdeReader<D> {
    type Item = Result<D, DeError>;

    fn next(&mut self) -> Option<Self::Item> {
        let (record, headers) = self.inner.next_record_with_headers()?;
        Some(deserialize_record(&record, headers, Some(self.dialect.clone())))
    }
}
