#[cfg(test)]
mod tests {
    use crate::{default_dialect, SerdeReader};
    use crate::Parser;
    use std::fs::File;
    use std::io::{Write};
    use serde::Deserialize;
    use crate::aligned_buffer::AlignedBuffer;
    use simd_csv::ZeroCopyReader;

    fn reader_from_str(s: &str) -> AlignedBuffer {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(s.as_bytes()).unwrap();
        f.flush().unwrap();
        AlignedBuffer::new(&f.reopen().unwrap()).unwrap()
    }

    #[test]
    fn test_line_parsing() {
        let line = "1,2,30,\"300, 400\",4\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let record = p.read_line().unwrap();
        assert_eq!(record, vec!["1", "2", "30", "\"300, 400\"",  "4"])
    }

    #[test]
    fn test_last_line_without_newline() {
        let mut p = Parser::new(default_dialect(), reader_from_str("a,b\n1,2\n3,4"));
        assert_eq!(p.read_line().unwrap(), vec!["a", "b"]);
        assert_eq!(p.read_line().unwrap(), vec!["1", "2"]);
        assert_eq!(p.read_line().unwrap(), vec!["3", "4"]);
        assert!(p.read_line().is_none());
    }

    #[test]
    fn test_multi_line_parsing() {
        let line = "1,2,30,\"300, 400\",4\n\
        1,2,30,\"300, 400\",4\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let record = p.read_line().unwrap();
        assert_eq!(record, vec!["1", "2", "30", "\"300, 400\"",  "4"]);
        let record = p.read_line().unwrap();
        assert_eq!(record, vec!["1", "2", "30", "\"300, 400\"",  "4"]);
    }


    #[test]
    fn test_line_parsing_boundaries() {
        let line = "12345678910,12345678910,12345678910,12345678910,offscore blah blah,season\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let record = p.read_line().unwrap();
        dbg!(&record);
        assert_eq!(str::from_utf8(&record[record.len() -2]).unwrap(), "offscore blah blah")
    }

    #[test]
    fn test_line_parsing_boundaries_garbage() {
        let line = "12345678910,12345678910,12345678910,12345678910,offscore blah blah,season\nblah, \n\"";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let record = p.read_line().unwrap();
        dbg!(&record);
        assert_eq!(str::from_utf8(&record[record.len() -2]).unwrap(), "offscore blah blah")
    }


    #[test]
    fn test_line_parsing_nfl_1() {
        let line = "20120905_DAL@NYG,1,,0,DAL,NYG,,,,D.Bailey kicks 69 yards from DAL 35 to NYG -4. D.Wilson to NYG 16 for 20 yards (A.Holmes).,0,0,2012\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let record = p.read_line().unwrap();
        assert_eq!(str::from_utf8(&record[record.len() -4]).unwrap(), "D.Bailey kicks 69 yards from DAL 35 to NYG -4. D.Wilson to NYG 16 for 20 yards (A.Holmes).".to_string())
    }

    #[test]
    fn test_line_parsing_nfl_2() {
        let line = "20120905_DAL@NYG,1,59,49,NYG,DAL,2,10,84,(14:49) E.Manning pass short middle to V.Cruz to NYG 21 for 5 yards (S.Lee) [J.Hatcher].,0,0,2012\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let record = p.read_line().unwrap();
        assert_eq!(str::from_utf8(&record[record.len() -4]).unwrap(), "(14:49) E.Manning pass short middle to V.Cruz to NYG 21 for 5 yards (S.Lee) [J.Hatcher].".to_string())
    }

    #[test]
    fn test_line_parsing_nfl_3() {
        let line = "20120905_DAL@NYG,1,57,9,NYG,DAL,1,10,87,(12:09) A.Bradshaw left tackle to NYG 15 for 2 yards (J.Hatcher J.Price-Brent).,0,0,2012\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let record = p.read_line().unwrap();
        assert_eq!(str::from_utf8(&record[record.len() -4]).unwrap(), "(12:09) A.Bradshaw left tackle to NYG 15 for 2 yards (J.Hatcher J.Price-Brent).");
        assert_eq!(str::from_utf8(&record[record.len() -1]).unwrap(), "2012");
    }

    #[test]
    fn test_line_parsing_nfl_4() {
        let line = "20120905_DAL@NYG,1,57,9,NYG,DAL,1,10,87,(12:09) A.Bradshaw left tackle to NYG 15 for 2 yards (J.Hatcher J.Price-Brent).,0,0,2012\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let record = p.read_line().unwrap();
        assert_eq!(str::from_utf8(&record[record.len() -4]).unwrap(), "(12:09) A.Bradshaw left tackle to NYG 15 for 2 yards (J.Hatcher J.Price-Brent).".to_string());
        assert_eq!(str::from_utf8(&record[record.len() -1]).unwrap(), "2012");
    }

    #[test]
    fn test_line_parsing_nfl_nested_quotes() {
        let line = "20120923_TB@DAL,3,29,12,TB,DAL,3,8,78,\"(14:12) (Shotgun) J.Freeman pass incomplete deep left to D.Clark. Pass incomplete on a \"\"seam\"\" route; Carter closest defender.\",7,10,2012\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let record = p.read_line().unwrap();
        assert_eq!(str::from_utf8(&record[record.len() -4]).unwrap(), "\"(14:12) (Shotgun) J.Freeman pass incomplete deep left to D.Clark. Pass incomplete on a \"\"seam\"\" route; Carter closest defender.\"".to_string());
        assert_eq!(str::from_utf8(&record[record.len() -1]).unwrap(), "2012");
    }

    /// A CSV whose bytes end at, or just short of, a page boundary.
    ///
    /// `classify` always loads a full `CHUNK_SIZE` block, so the load covering
    /// the last line of such a file reaches past the end of the mapping.
    /// Without the zero-padded tail chunk this faults rather than failing.
    #[test]
    fn test_parse_file_ending_at_page_boundary() {
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as usize;
        // Sweep the window in which the final block load can cross the end of
        // the mapping, so the case is hit wherever the last line starts.
        for slack in 0..64 {
            let total = 2 * page - slack;
            let content = csv_of_exactly(total);
            assert_eq!(content.len(), total);

            let expected: Vec<Vec<&str>> = content
                .strip_suffix('\n')
                .unwrap()
                .split('\n')
                .map(|line| line.split(',').collect())
                .collect();

            let mut p = Parser::new(default_dialect(), reader_from_str(&content));
            let mut seen = 0;
            while let Some(record) = p.read_line() {
                assert!(seen < expected.len(), "{total}-byte file: too many records");
                assert_eq!(record, expected[seen], "{total}-byte file, record {seen}");
                seen += 1;
            }
            assert_eq!(seen, expected.len(), "{total}-byte file: records parsed");
        }
    }

    /// A newline-terminated CSV of exactly `total` bytes. The final row is
    /// padded so the file ends on the requested byte, wherever that falls.
    fn csv_of_exactly(total: usize) -> String {
        const ROW: &str = "aaaa,bbbb,cccc,dddd\n";
        assert!(total >= 2 * ROW.len());
        let mut s = String::with_capacity(total);
        while s.len() + 2 * ROW.len() <= total {
            s.push_str(ROW);
        }
        // Leaves between ROW.len() and 2*ROW.len() bytes for the final row.
        let remaining = total - s.len();
        s.push_str("z,");
        s.extend(std::iter::repeat('q').take(remaining - 3));
        s.push('\n');
        s
    }

    #[test]
    fn test_parse_file() {
        let file = File::open("examples/customers-2000000.csv").unwrap();
        let mut p = Parser::new(default_dialect(), AlignedBuffer::new(&file).unwrap());
        while let Some(mut record) = p.read_line() {
            for field in record.iter() {
                let _ = field.len();
            }
        }
    }

    #[test]
    fn bench_parse_file_profile() {
        fn parse_file() {
            let file = File::open("examples/nfl.csv").unwrap();
            let mut p = Parser::new(default_dialect(), AlignedBuffer::new(&file).unwrap());
            while let Some(mut record) = p.read_line() {
                for field in record.iter() {
                    let _ = field.len();
                }
            }
        }

        for _ in 0..100 {
            parse_file();
        }

    }

    #[test]
    fn test_equality_simd_csv() {
        for path in ["examples/customers-2000000.csv", "examples/nfl.csv", "examples/EDW.TEST_CAL_DT.csv"].iter() {
            let file = File::open(path).unwrap();
            let mut p = Parser::new(default_dialect(), AlignedBuffer::new(&file).unwrap());
            let file2 = File::open(path).unwrap();
            let mut reader = ZeroCopyReader::from_reader(file2);
            p.read_line(); // skip header
            let mut counter = 0;
            while let Some(theirs) = reader.read_byte_record().unwrap() {
                if let Some(ours) = p.read_line() {
                    counter += 1;
                    for i in 0..ours.len() {
                        let o = str::from_utf8(&ours[i]).unwrap();
                        let theirs = str::from_utf8(&theirs[i]).unwrap();
                        if *o != *theirs {
                            dbg!(counter, theirs, &ours);
                        }
                        assert_eq!(*o, *theirs, "Mismatch at record {}, field {}", counter, i);
                    }
                    assert_eq!(ours.len(), theirs.len(), "Mismatch in number of fields at record {}, {} vs {}", counter, ours.len(), theirs.len());
                } else {
                    panic!("Mismatch in number of records ours, at line {}, file {}", counter, path);
                }
            }
        }
    }

    #[test]
    fn test_equality_serde_csv_normal_escapes() {
        #[derive(Debug, Deserialize, PartialEq)]
        #[allow(dead_code)] // not reading anything, just deserializing
        struct Play {
            gameid: String,
            qtr: u64,
            off: String,
            def: String,
            description: String,
            offscore: u64,
            defscore: u64,
            season: u64,
        }
        let path = "examples/nfl.csv";
        let file = File::open(path).unwrap();
        let mut our_reader = SerdeReader::<Play>::new(default_dialect(), &file);
        let file2 = File::open(path).unwrap();
        let mut csv_reader = csv::ReaderBuilder::new().escape(Some(b'"')).from_reader(file2);
        let mut counter = 0;
        for (theirs, ours) in csv_reader.deserialize::<Play>().zip(our_reader.into_iter()) {
            assert_eq!(theirs.unwrap(), ours.unwrap(), "Mismatch at record {}", counter);
            counter += 1;
        }
    }
}