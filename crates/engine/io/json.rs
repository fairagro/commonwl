use serde::Serialize;
use serde_json::ser::Formatter;
use std::io;

struct PythonLikeFormatter;

impl Formatter for PythonLikeFormatter {
    fn begin_array_value<W: io::Write + ?Sized>(
        &mut self,
        writer: &mut W,
        first: bool,
    ) -> io::Result<()> {
        if first {
            Ok(())
        } else {
            writer.write_all(b", ")
        }
    }

    fn begin_object_key<W: io::Write + ?Sized>(
        &mut self,
        writer: &mut W,
        first: bool,
    ) -> io::Result<()> {
        if first {
            Ok(())
        } else {
            writer.write_all(b", ")
        }
    }

    fn begin_object_value<W: io::Write + ?Sized>(&mut self, writer: &mut W) -> io::Result<()> {
        writer.write_all(b": ")
    }
}
pub(crate) fn to_string_dump<T: Serialize>(value: &T) -> serde_json::Result<String> {
    let mut buf = Vec::new();
    let mut ser = serde_json::ser::Serializer::with_formatter(&mut buf, PythonLikeFormatter);
    value.serialize(&mut ser)?;
    Ok(String::from_utf8(buf).unwrap())
}
