#[cfg(test)]
mod tests {
    use crate::FieldBuffer;
use crate::default_dialect;
use crate::Parser;
use std::fs::File;
    use std::io::{BufReader, Cursor};
    use super::*;
    use test::Bencher;
    fn reader_from_str(s: &str) -> BufReader<Cursor<&[u8]>> {
        BufReader::new(Cursor::new(s.as_bytes()))
    }

    #[test]
    fn test_line_parsing() {
        let line = "1,2,30,\"300, 400\",4\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let result = p.read_line();
        assert_eq!(result, vec!["1".to_string(), "2".to_string(), "30".to_string(), "300, 400".to_string(),  "4".to_string()])
    }

    #[test]
    fn test_line_parsing_continuation() {
        let line = ", \",1,2,\"300, 400\",4\n";
        let mut p = Parser {
            dialect: default_dialect(),
            inside_quotes: true,
            bufreader: reader_from_str(line),
            field_buffer: FieldBuffer::new(default_dialect()),
        };
        let result = p.read_line();
        assert_eq!(result, vec![", \"".to_string(), "1".to_string(), "2".to_string(), "300, 400".to_string(),  "4".to_string()])
    }

    #[test]
    fn test_line_parsing_escaped_newlines() {
        let line = ", \",1,2,\"300,\r\n 400\",4\n";
        let mut p = Parser {
            dialect: default_dialect(),
            inside_quotes: true,
            bufreader: reader_from_str(line),
            field_buffer: FieldBuffer::new(default_dialect()),
        };
        let result = p.read_line();
        assert_eq!(result, vec![", \"".to_string(), "1".to_string(), "2".to_string(), "300,\r\n 400".to_string(),  "4".to_string()])
    }

    #[test]
    fn test_line_parsing_boundaries() {
        let line = "12345678910,12345678910,12345678910,12345678910,offscore blah blah,season\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let result = p.read_line();
        println!("{:?}", result);
        assert_eq!(result[result.len() -2], "offscore blah blah".to_string())
    }


    #[test]
    fn test_line_parsing_nfl_1() {
        let line = "20120905_DAL@NYG,1,,0,DAL,NYG,,,,D.Bailey kicks 69 yards from DAL 35 to NYG -4. D.Wilson to NYG 16 for 20 yards (A.Holmes).,0,0,2012\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let result = p.read_line();
        println!("{:?}", result);
        assert_eq!(result[result.len()-4], "D.Bailey kicks 69 yards from DAL 35 to NYG -4. D.Wilson to NYG 16 for 20 yards (A.Holmes).".to_string())
    }

    #[test]
    fn test_line_parsing_nfl_2() {
        let line = "20120905_DAL@NYG,1,59,49,NYG,DAL,2,10,84,(14:49) E.Manning pass short middle to V.Cruz to NYG 21 for 5 yards (S.Lee) [J.Hatcher].,0,0,2012\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let result = p.read_line();
        println!("{:?}", result);
        assert_eq!(result[result.len()-4], "(14:49) E.Manning pass short middle to V.Cruz to NYG 21 for 5 yards (S.Lee) [J.Hatcher].".to_string())
    }

    #[test]
    fn test_line_parsing_nfl_3() {
        let line = "20120905_DAL@NYG,1,57,9,NYG,DAL,1,10,87,(12:09) A.Bradshaw left tackle to NYG 15 for 2 yards (J.Hatcher J.Price-Brent).,0,0,2012\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let result = p.read_line();
        println!("{:?}", result);
        assert_eq!(result[result.len()-4], "(12:09) A.Bradshaw left tackle to NYG 15 for 2 yards (J.Hatcher J.Price-Brent).".to_string());
        assert_eq!(result[result.len()-1], "2012");
    }

    #[test]
    fn test_line_parsing_nfl_4() {
        let line = "20120905_DAL@NYG,1,57,9,NYG,DAL,1,10,87,(12:09) A.Bradshaw left tackle to NYG 15 for 2 yards (J.Hatcher J.Price-Brent).,0,0,2012\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let result = p.read_line();
        println!("{:?}", result);
        assert_eq!(result[result.len()-4], "(12:09) A.Bradshaw left tackle to NYG 15 for 2 yards (J.Hatcher J.Price-Brent).".to_string());
        assert_eq!(result[result.len()-1], "2012");
    }

    #[test]
    fn test_line_parsing_nfl_nested_quotes() {
        let line = "20120923_TB@DAL,3,29,12,TB,DAL,3,8,78,\"(14:12) (Shotgun) J.Freeman pass incomplete deep left to D.Clark. Pass incomplete on a \"\"seam\"\" route; Carter closest defender.\",7,10,2012\n";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let result = p.read_line();
        assert_eq!(result[result.len()-4], "(14:12) (Shotgun) J.Freeman pass incomplete deep left to D.Clark. Pass incomplete on a \"seam\" route; Carter closest defender.".to_string());
        assert_eq!(result[result.len()-1], "2012");
    }

    #[test]
    fn test_delimiter_masking() {
        let line = "";
        let mut p = Parser::new(default_dialect(), reader_from_str(line));
        let mask = Parser::<File>::mask_invalid_bytes(16);
        assert_eq!(mask & 1 << 17 , 0);
    }
    #[test]
    fn test_parse_file() {
        let file = File::open("examples/nfl.csv").unwrap();
        let p = Parser::new(default_dialect(), BufReader::new(file));
        for (idx, line) in p.enumerate() {
            let _ = line;
            assert_ne!(line.len(), 0);

        }
    }

    #[bench]
    fn bench_parse_file(b: &mut Bencher) {
        fn parse_file(){
            let file = File::open("examples/nfl.csv").unwrap();
            let p = Parser::new(default_dialect(), BufReader::new(file));
            for line in p {
                let _ = line;
            }
        }
        b.iter(|| parse_file());
    }

    #[bench]
    fn bench_parse_line(b: &mut Bencher) {
        let file = File::open("examples/nfl.csv").unwrap();
        let p = Parser::new(default_dialect(), BufReader::new(file));
        let mut pp = p.into_iter();
        b.iter(|| pp.next());
    }


    #[bench]
    fn bench_parse_file_simd_csv(b: &mut Bencher) {
        use simd_csv::{Reader, ByteRecord};
        fn parse_file(){
            let file = File::open("examples/nfl.csv").unwrap();

            let mut reader = Reader::from_reader(file);
            let mut record = ByteRecord::new();

            while reader.read_byte_record(&mut record).unwrap() {
                for cell in record.iter() {
                    let _ = String::from_utf8(cell.to_vec()).unwrap();
                }
            }
        }
        b.iter(|| parse_file());
    }


}