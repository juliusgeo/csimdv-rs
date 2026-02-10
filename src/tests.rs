#[cfg(test)]
mod tests {
    use crate::default_dialect;
    use crate::Parser;
    use std::fs::File;
    use std::io::Cursor;
    use crate::aligned_buffer::AlignedBuffer;
    use simd_csv::ZeroCopyReader;

    fn reader_from_str(s: &str) -> AlignedBuffer<Cursor<&[u8]>> {
        AlignedBuffer::new(
        Cursor::new(s.as_bytes())
        )
    }

    #[test]
    fn test_line_parsing() {
        let line = "1,2,30,\"300, 400\",4\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let record = p.read_line().unwrap();
        assert_eq!(record, vec!["1", "2", "30", "\"300, 400\"",  "4"])
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
        assert_eq!(&record[record.len() -2], "offscore blah blah")
    }

    #[test]
    fn test_line_parsing_boundaries_garbage() {
        let line = "12345678910,12345678910,12345678910,12345678910,offscore blah blah,season\nblah, \n\"";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let record = p.read_line().unwrap();
        dbg!(&record);
        assert_eq!(&record[record.len() -2], "offscore blah blah")
    }


    #[test]
    fn test_line_parsing_nfl_1() {
        let line = "20120905_DAL@NYG,1,,0,DAL,NYG,,,,D.Bailey kicks 69 yards from DAL 35 to NYG -4. D.Wilson to NYG 16 for 20 yards (A.Holmes).,0,0,2012\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let record = p.read_line().unwrap();
        assert_eq!(record[record.len()-4], "D.Bailey kicks 69 yards from DAL 35 to NYG -4. D.Wilson to NYG 16 for 20 yards (A.Holmes).".to_string())
    }

    #[test]
    fn test_line_parsing_nfl_2() {
        let line = "20120905_DAL@NYG,1,59,49,NYG,DAL,2,10,84,(14:49) E.Manning pass short middle to V.Cruz to NYG 21 for 5 yards (S.Lee) [J.Hatcher].,0,0,2012\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let record = p.read_line().unwrap();
        assert_eq!(record[record.len()-4], "(14:49) E.Manning pass short middle to V.Cruz to NYG 21 for 5 yards (S.Lee) [J.Hatcher].".to_string())
    }

    #[test]
    fn test_line_parsing_nfl_3() {
        let line = "20120905_DAL@NYG,1,57,9,NYG,DAL,1,10,87,(12:09) A.Bradshaw left tackle to NYG 15 for 2 yards (J.Hatcher J.Price-Brent).,0,0,2012\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let record = p.read_line().unwrap();
        assert_eq!(&record[record.len()-4], "(12:09) A.Bradshaw left tackle to NYG 15 for 2 yards (J.Hatcher J.Price-Brent).");
        assert_eq!(&record[record.len()-1], "2012");
    }

    #[test]
    fn test_line_parsing_nfl_4() {
        let line = "20120905_DAL@NYG,1,57,9,NYG,DAL,1,10,87,(12:09) A.Bradshaw left tackle to NYG 15 for 2 yards (J.Hatcher J.Price-Brent).,0,0,2012\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let record = p.read_line().unwrap();
        assert_eq!(&record[record.len()-4], "(12:09) A.Bradshaw left tackle to NYG 15 for 2 yards (J.Hatcher J.Price-Brent).".to_string());
        assert_eq!(&record[record.len()-1], "2012");
    }

    #[test]
    fn test_line_parsing_nfl_nested_quotes() {
        let line = "20120923_TB@DAL,3,29,12,TB,DAL,3,8,78,\"(14:12) (Shotgun) J.Freeman pass incomplete deep left to D.Clark. Pass incomplete on a \"\"seam\"\" route; Carter closest defender.\",7,10,2012\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let record = p.read_line().unwrap();
        assert_eq!(&record[record.len()-4], "\"(14:12) (Shotgun) J.Freeman pass incomplete deep left to D.Clark. Pass incomplete on a \"\"seam\"\" route; Carter closest defender.\"".to_string());
        assert_eq!(&record[record.len()-1], "2012");
    }

    #[test]
    fn test_parse_file() {
        let file = File::open("examples/customers-2000000.csv").unwrap();
        let mut p = Parser::new(default_dialect(), AlignedBuffer::new(file));
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
            let mut p = Parser::new(default_dialect(), AlignedBuffer::new(file));
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
        for path in ["examples/customers-2000000.csv", "examples/nfl.csv"].iter() {
            let file = File::open(path).unwrap();
            let mut p = Parser::new(default_dialect(), AlignedBuffer::new(file));
            let file2 = File::open(path).unwrap();
            let mut reader = ZeroCopyReader::from_reader(file2);
            p.read_line(); // skip header
            let mut counter = 0;
            while let Some(ours) = p.read_line() {
                if let Some(theirs) = reader.read_byte_record().unwrap() {
                    counter += 1;
                    assert_eq!(ours.len(), theirs.len(), "Mismatch in number of fields at record {}", counter);
                    for i in 0..ours.len() {
                        let o = &ours[i];
                        let theirs = str::from_utf8(&theirs[i]).unwrap();
                        assert_eq!(*o, *theirs);
                    }
                } else {
                    panic!("Mismatch in number of records");
                }
            }
        }
    }
}