//! Serde deserialization of a parsed `Record` into a user-defined struct.
use std::error::Error as StdError;
use std::fmt;
use std::num;

use serde::de::value::BorrowedStrDeserializer;
use serde::de::{
    Deserialize, DeserializeSeed, Deserializer, Error as SerdeError, MapAccess, SeqAccess, Visitor,
};
use crate::Dialect;
use crate::record::Record;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Headers {
    fields: Vec<String>,
}

impl Headers {
    pub fn from_record(record: &Record<'_>) -> Self {
        Headers { fields: (0..record.len()).map(|i| str::from_utf8(&record[i]).unwrap().to_string() ).collect() }
    }

    pub fn len(&self) -> usize {
        self.fields.len()
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    pub fn get(&self, i: usize) -> Option<&str> {
        self.fields.get(i).map(|s| s.as_str())
    }
}

pub fn deserialize_record<'de, D: Deserialize<'de>>(
    record: &'de Record<'de>,
    headers: Option<&'de Headers>,
    dialect: Option<crate::Dialect>
) -> Result<D, DeError> {
    let mut de = RecordDeserializer::new(record, headers, dialect);
    D::deserialize(&mut de)
}


pub struct RecordDeserializer<'de> {
    record: &'de Record<'de>,
    headers: Option<&'de Headers>,
    field: usize,
    header: usize,
    dialect: Option<Dialect>,
    escape_combo: String,
}

impl<'de> RecordDeserializer<'de> {
    pub fn new(record: &'de Record<'de>, headers: Option<&'de Headers>, dialect: Option<crate::Dialect>) -> Self {
        if dialect.is_some() {
            let combo = format!("{}{}", dialect.unwrap().escape_char, dialect.unwrap().quotechar);
            RecordDeserializer { record, headers, field: 0, header: 0, dialect: dialect, escape_combo: combo }
        } else {
            RecordDeserializer { record, headers, field: 0, header: 0, dialect: dialect, escape_combo: "".to_string() }
        }
    }

    fn has_headers(&self) -> bool {
        self.headers.is_some()
    }

    fn next_field(&mut self) -> Result<&'de [u8], DeError> {
        // Indexing through `&'de Record` reborrows for `'de`, so fields are
        // borrowed straight out of the mmap -- no copy, no allocation.
        match self.peek_field() {
            Some(field) => {
                self.field += 1;
                Ok(field)
            }
            None => Err(DeError::UnexpectedEndOfRow { field: self.field }),
        }
    }

    fn peek_field(&self) -> Option<&'de [u8]> {
        if self.field < self.record.len() { Some(&self.record[self.field]) } else { None }
    }

    fn last_field(&self) -> usize {
        self.field.saturating_sub(1)
    }

    fn next_header(&mut self) -> Option<&'de str> {
        let header = self.headers?.get(self.header)?;
        self.header += 1;
        Some(header)
    }
}

macro_rules! deserialize_int {
    ($($method:ident => $visit:ident : $ty:ty),* $(,)?) => {
        $(
            fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
                let field = str::from_utf8(self.next_field()?).unwrap();
                let parsed = match field.strip_prefix("0x") {
                    Some(digits) => <$ty>::from_str_radix(digits, 16),
                    None => field.parse::<$ty>(),
                };
                match parsed {
                    Ok(n) => visitor.$visit(n),
                    Err(source) => Err(DeError::ParseInt { field: self.last_field(), source }),
                }
            }
        )*
    };
}



macro_rules! deserialize_float {
    ($($method:ident => $visit:ident : $ty:ty),* $(,)?) => {
        $(
            fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
                let field = str::from_utf8(self.next_field()?).unwrap();
                match field.parse::<$ty>() {
                    Ok(n) => visitor.$visit(n),
                    Err(source) => Err(DeError::ParseFloat { field: self.last_field(), source }),
                }
            }
        )*
    };
}

macro_rules! unsupported {
    ($($method:ident $(($($arg:ident: $ty:ty),*))?),* $(,)?) => {
        $(
            fn $method<V: Visitor<'de>>(
                self,
                $($($arg: $ty,)*)?
                _visitor: V,
            ) -> Result<V::Value, DeError> {
                Err(DeError::Unsupported(stringify!($method)))
            }
        )*
    };
}

impl<'a, 'de: 'a> Deserializer<'de> for &'a mut RecordDeserializer<'de> {
    type Error = DeError;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        // Values captured by #[serde(flatten)] arrive here, so unescape like deserialize_string.
        self.deserialize_string(visitor)
    }

    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        // serde_derive calls this instead of deserialize_struct when a struct has a flatten field.
        if self.has_headers() { visitor.visit_map(self) } else { Err(DeError::Unsupported("deserialize_map without headers")) }
    }
    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        let field = self.next_field()?;
        visitor.visit_borrowed_str(str::from_utf8(field).unwrap())
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        let mut field = str::from_utf8(self.next_field()?).unwrap();
        match self.dialect {
            Some(d) => {
                if d.escape == true {
                    if field.starts_with(&d.quotechar.to_string()) {
                        let mut field = field.replace(&self.escape_combo, &d.quotechar.to_string());
                        visitor.visit_str(&field[1..field.len()-1])
                    } else {
                        visitor.visit_str(field)
                    }
                } else {
                    visitor.visit_str(field)
                }
            }
            None => {
                visitor.visit_str(field)
            }
        }
    }

    /// An empty field, or a row that ran out of fields, is `None`.
    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        match self.peek_field() {
            None => visitor.visit_none(),
            Some(s) => {
                if s.len() == 0 {
                    self.next_field()?;
                    visitor.visit_none()
                } else {
                    visitor.visit_some(self)
                }
            },
        }
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, DeError> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        visitor.visit_seq(self)
    }

    fn deserialize_tuple<V: Visitor<'de>>(
        self,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, DeError> {
        visitor.visit_seq(self)
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, DeError> {
        visitor.visit_seq(self)
    }

    
    
    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, DeError> {
        if self.has_headers() { visitor.visit_map(self) } else { visitor.visit_seq(self) }
    }

    
    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        self.next_field()?;
        visitor.visit_unit()
    }

    deserialize_int!(
        deserialize_i8 => visit_i8: i8,
        deserialize_i16 => visit_i16: i16,
        deserialize_i32 => visit_i32: i32,
        deserialize_i64 => visit_i64: i64,
        deserialize_i128 => visit_i128: i128,
        deserialize_u8 => visit_u8: u8,
        deserialize_u16 => visit_u16: u16,
        deserialize_u32 => visit_u32: u32,
        deserialize_u64 => visit_u64: u64,
        deserialize_u128 => visit_u128: u128,
    );

    deserialize_float!(
        deserialize_f32 => visit_f32: f32,
        deserialize_f64 => visit_f64: f64,
    );

    unsupported!(
        // Not implemented yet, but meaningful for CSV: both are a
        // `next_field()` plus a parse, in the shape of `deserialize_int!`.
        deserialize_bool,
        deserialize_char,
        // No CSV meaning: a row is a flat list of text fields, so there is
        // nothing for a unit or a nested enum to read.
        deserialize_unit,
        deserialize_unit_struct(_name: &'static str),
        deserialize_enum(_name: &'static str, _variants: &'static [&'static str]),
        // Fields are UTF-8: `Record` validates on access, so a field can
        // always be handed over as `&str` and never needs a byte path.
        deserialize_bytes,
        deserialize_byte_buf,
        // Unreachable: map keys go through `BorrowedStrDeserializer` in
        // `next_key_seed`, never through this deserializer.
        deserialize_identifier,
    );
}



impl<'a, 'de: 'a> SeqAccess<'de> for &'a mut RecordDeserializer<'de> {
    type Error = DeError;

    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, DeError> {
        if self.peek_field().is_none() {
            return Ok(None);
        }
        seed.deserialize(&mut **self).map(Some)
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.record.len() - self.field)
    }
}


impl<'a, 'de: 'a> MapAccess<'de> for &'a mut RecordDeserializer<'de> {
    type Error = DeError;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, DeError> {
        let header = match self.next_header() {
            Some(header) => header,
            None => return Ok(None),
        };
        seed.deserialize(BorrowedStrDeserializer::new(header)).map(Some)
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, DeError> {
        seed.deserialize(&mut **self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeError {
    
    
    Message(String),
    
    UnexpectedEndOfRow { field: usize },
    
    Unsupported(&'static str),
    
    ParseInt { field: usize, source: num::ParseIntError },
    
    ParseFloat { field: usize, source: num::ParseFloatError },
}

impl SerdeError for DeError {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        DeError::Message(msg.to_string())
    }
}

impl fmt::Display for DeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeError::Message(msg) => write!(f, "{}", msg),
            DeError::UnexpectedEndOfRow { field } => {
                write!(f, "expected a field at index {} but the record ended", field)
            }
            DeError::Unsupported(method) => {
                write!(f, "{} is not supported by this deserializer", method)
            }
            DeError::ParseInt { field, source } => {
                write!(f, "field {}: {}", field, source)
            }
            DeError::ParseFloat { field, source } => {
                write!(f, "field {}: {}", field, source)
            }
        }
    }
}

impl StdError for DeError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            DeError::ParseInt { source, .. } => Some(source),
            DeError::ParseFloat { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aligned_buffer::AlignedBuffer;
    use crate::{Parser, default_dialect};
    use serde::Deserialize;
    use std::io::Write;

    
    #[derive(Debug, Deserialize, PartialEq)]
    struct Row {
        city: String,
        state: String,
        country: String,
    }

    
    #[derive(Debug, Deserialize, PartialEq)]
    struct BorrowedRow<'a> {
        city: &'a str,
        state: &'a str,
        country: &'a str,
        // Not `&'a u64`: borrowing only works for types that alias the input
        // bytes (`&str`, `&[u8]`). A `u64` is produced by parsing, so there is
        // nothing in the mmap to point at.
        population: u64,
    }

    const DATA: &str = "city,state,country,population\n\
                        Portland,Oregon,USA,1000\n\
                        Austin,Texas,USA,20000\n";

    fn reader_from_str(s: &str) -> AlignedBuffer {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(s.as_bytes()).unwrap();
        f.flush().unwrap();
        AlignedBuffer::new(&f.reopen().unwrap()).unwrap()
    }

    #[test]
    fn test_deserialize_struct_by_header() {
        let mut p = Parser::new(default_dialect(), reader_from_str(DATA));
        let headers = Headers::from_record(&p.read_line().unwrap());
        let record = p.read_line().unwrap();
        let row: Row = deserialize_record(&record, Some(&headers), None).unwrap();
        assert_eq!(
            row,
            Row {
                city: "Portland".to_string(),
                state: "Oregon".to_string(),
                country: "USA".to_string(),
            }
        );
    }

    #[test]
    fn test_deserialize_all_rows() {
        let mut p = Parser::new(default_dialect(), reader_from_str(DATA));
        let headers = Headers::from_record(&p.read_line().unwrap());
        let mut rows = Vec::new();
        while let Some(record) = p.read_line() {
            rows.push(deserialize_record::<Row>(&record, Some(&headers), None).unwrap());
        }
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].city, "Portland");
        assert_eq!(rows[1].city, "Austin");
        assert_eq!(rows[1].state, "Texas");
    }

    
    #[test]
    fn test_deserialize_borrowed() {
        let mut p = Parser::new(default_dialect(), reader_from_str(DATA));
        let headers = Headers::from_record(&p.read_line().unwrap());
        let record = p.read_line().unwrap();
        let row: BorrowedRow<'_> = deserialize_record(&record, Some(&headers), None).unwrap();
        assert_eq!(
            row,
            BorrowedRow {
                city: "Portland",
                state: "Oregon",
                country: "USA",
                population: 1000,
            }
        );
    }

    
    
    #[test]
    fn test_column_order_independent() {
        let data = "country,city,state\nUSA,Portland,Oregon\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(data));
        let headers = Headers::from_record(&p.read_line().unwrap());
        let record = p.read_line().unwrap();
        let row: Row = deserialize_record(&record, Some(&headers), None).unwrap();
        assert_eq!(row.city, "Portland");
        assert_eq!(row.country, "USA");
    }

    
    
    #[test]
    fn test_extra_column_ignored() {
        let data = "city,state,country,zip\nPortland,Oregon,USA,97201\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(data));
        let headers = Headers::from_record(&p.read_line().unwrap());
        let record = p.read_line().unwrap();
        let row: Row = deserialize_record(&record, Some(&headers), None).unwrap();
        assert_eq!(row.state, "Oregon");
    }

    
    #[test]
    fn test_deserialize_without_headers() {
        let data = "Portland,Oregon,USA\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(data));
        let record = p.read_line().unwrap();
        let row: Row = deserialize_record(&record, None,  None).unwrap();
        assert_eq!(row.city, "Portland");
        assert_eq!(row.country, "USA");
    }

    #[test]
    fn test_deserialize_vec_of_strings() {
        let data = "Portland,Oregon,USA\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(data));
        let record = p.read_line().unwrap();
        let row: Vec<String> = deserialize_record(&record, None, None).unwrap();
        assert_eq!(row, vec!["Portland", "Oregon", "USA"]);
    }

    #[test]
    fn test_missing_field_is_an_error() {
        let data = "city,state\nPortland,Oregon\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(data));
        let headers = Headers::from_record(&p.read_line().unwrap());
        let record = p.read_line().unwrap();
        let err = deserialize_record::<Row>(&record, Some(&headers), None).unwrap_err();
        assert!(matches!(err, DeError::Message(_)), "got {:?}", err);
    }

    #[test]
    fn test_flatten_collects_extra_columns() {
        #[derive(Debug, Deserialize, PartialEq)]
        struct Flat {
            id: u32,
            #[serde(flatten)]
            rest: std::collections::HashMap<String, String>,
        }
        let data = "id,extRarity,extText\n1,Rare,\"a, \"\"b\"\"\"\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(data));
        let headers = Headers::from_record(&p.read_line().unwrap());
        let record = p.read_line().unwrap();
        let row: Flat = deserialize_record(&record, Some(&headers), Some(default_dialect())).unwrap();
        assert_eq!(row.id, 1);
        assert_eq!(row.rest["extRarity"], "Rare");
        assert_eq!(row.rest["extText"], "a, \"b\"");
    }

    // ---- ints and floats ----

    #[derive(Debug, Deserialize, PartialEq)]
    struct Measurement {
        station: String,
        count: u32,
        delta: i32,
        temp: f64,
    }

    #[test]
    fn test_deserialize_ints_and_floats() {
        let data = "station,count,delta,temp\n\
                    PDX,1200,-45,12.5\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(data));
        let headers = Headers::from_record(&p.read_line().unwrap());
        let record = p.read_line().unwrap();
        let row: Measurement = deserialize_record(&record, Some(&headers), None).unwrap();
        assert_eq!(
            row,
            Measurement {
                station: "PDX".to_string(),
                count: 1200,
                delta: -45,
                temp: 12.5,
            }
        );
    }

    
    #[test]
    fn test_integer_widths() {
        #[derive(Debug, Deserialize, PartialEq)]
        struct Widths {
            a: i8,
            b: i16,
            c: i32,
            d: i64,
            e: i128,
            f: u8,
            g: u16,
            h: u32,
            i: u64,
            j: u128,
        }
        let data = "a,b,c,d,e,f,g,h,i,j\n\
                    -128,-32768,-2147483648,-9223372036854775808,\
                    -170141183460469231731687303715884105728,\
                    255,65535,4294967295,18446744073709551615,\
                    340282366920938463463374607431768211455\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(data));
        let headers = Headers::from_record(&p.read_line().unwrap());
        let record = p.read_line().unwrap();
        let row: Widths = deserialize_record(&record, Some(&headers), None).unwrap();
        assert_eq!(row.a, i8::MIN);
        assert_eq!(row.b, i16::MIN);
        assert_eq!(row.c, i32::MIN);
        assert_eq!(row.d, i64::MIN);
        assert_eq!(row.e, i128::MIN);
        assert_eq!(row.f, u8::MAX);
        assert_eq!(row.g, u16::MAX);
        assert_eq!(row.h, u32::MAX);
        assert_eq!(row.i, u64::MAX);
        assert_eq!(row.j, u128::MAX);
    }

    #[test]
    fn test_hex_prefix() {
        #[derive(Debug, Deserialize, PartialEq)]
        struct Flags {
            mask: u32,
            plain: u32,
        }
        let data = "mask,plain\n0xff,255\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(data));
        let headers = Headers::from_record(&p.read_line().unwrap());
        let record = p.read_line().unwrap();
        let row: Flags = deserialize_record(&record, Some(&headers), None).unwrap();
        assert_eq!(row.mask, 255);
        assert_eq!(row.plain, 255);
    }

    #[test]
    fn test_float_forms() {
        #[derive(Debug, Deserialize, PartialEq)]
        struct Floats {
            small: f32,
            sci: f64,
            neg: f64,
            whole: f64,
        }
        let data = "small,sci,neg,whole\n1.5,1e5,-0.25,7\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(data));
        let headers = Headers::from_record(&p.read_line().unwrap());
        let record = p.read_line().unwrap();
        let row: Floats = deserialize_record(&record, Some(&headers), None).unwrap();
        assert_eq!(row.small, 1.5f32);
        assert_eq!(row.sci, 100000.0);
        assert_eq!(row.neg, -0.25);
        assert_eq!(row.whole, 7.0);
    }

    
    
    #[test]
    fn test_optional_numbers() {
        #[derive(Debug, Deserialize, PartialEq)]
        struct MaybeNum {
            name: String,
            count: Option<u32>,
            temp: Option<f64>,
        }
        let data = "name,count,temp\n\
                    PDX,,12.5\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(data));
        let headers = Headers::from_record(&p.read_line().unwrap());
        let record = p.read_line().unwrap();
        let row: MaybeNum = deserialize_record(&record, Some(&headers), None).unwrap();
        assert_eq!(row.count, None);
        assert_eq!(row.temp, Some(12.5));
    }

    #[test]
    fn test_numbers_without_headers() {
        let data = "PDX,1200,-45,12.5\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(data));
        let record = p.read_line().unwrap();
        let row: Measurement = deserialize_record(&record, None, None).unwrap();
        assert_eq!(row.count, 1200);
        assert_eq!(row.temp, 12.5);
    }

    #[test]
    fn test_deserialize_vec_of_ints() {
        let data = "1,2,3\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(data));
        let record = p.read_line().unwrap();
        let row: Vec<u32> = deserialize_record(&record, None, None).unwrap();
        assert_eq!(row, vec![1, 2, 3]);
    }

    
    #[test]
    fn test_int_parse_error_reports_field_index() {
        let data = "station,count,delta,temp\nPDX,oops,-45,12.5\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(data));
        let headers = Headers::from_record(&p.read_line().unwrap());
        let record = p.read_line().unwrap();
        let err = deserialize_record::<Measurement>(&record, Some(&headers), None).unwrap_err();
        assert!(matches!(err, DeError::ParseInt { field: 1, .. }), "got {:?}", err);
        assert!(err.to_string().starts_with("field 1: "), "got {}", err);
    }

    #[test]
    fn test_float_parse_error_reports_field_index() {
        let data = "station,count,delta,temp\nPDX,1200,-45,warm\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(data));
        let headers = Headers::from_record(&p.read_line().unwrap());
        let record = p.read_line().unwrap();
        let err = deserialize_record::<Measurement>(&record, Some(&headers), None).unwrap_err();
        assert!(matches!(err, DeError::ParseFloat { field: 3, .. }), "got {:?}", err);
    }

    
    #[test]
    fn test_integer_overflow_is_an_error() {
        let data = "station,count,delta,temp\nPDX,1200,99999999999,12.5\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(data));
        let headers = Headers::from_record(&p.read_line().unwrap());
        let record = p.read_line().unwrap();
        let err = deserialize_record::<Measurement>(&record, Some(&headers), None).unwrap_err();
        assert!(matches!(err, DeError::ParseInt { field: 2, .. }), "got {:?}", err);
    }
}
