use std::fmt;
use std::fmt::Write;

pub struct Position {
    filename: &'static str,
    offset: i32,
    line: i32,
    column: i32,
}

impl Position {
    pub fn is_valid(&self) -> bool {
        self.line > 0
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut s = String::from(self.filename);
        if self.is_valid() {
            if s != "" {
                s.push(':');
            }
        }
        s.push_str(&self.line.to_string());
        if self.column != 0 {
            write!(&mut s, ":{}", self.column).unwrap();
        }
        if s.is_empty() {
            s.push('-');
        }
        f.write_str(&s)
    }
}

pub struct File<'a> {
    set: &'a FileSet<'a>,
    name: &'static str,
    base: i32,
    size: i32,
    lines: Vec<i32>,
}

impl<'a> File<'a> {
    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn base(&self) -> i32 {
        self.base
    }

    pub fn size(&self) -> i32 {
        self.size
    }

    pub fn line_count(&self) -> i32 {
        self.lines.len()
    }
}


pub struct FileSet<'a> {
    base: i32,
    files: Vec<Box<File<'a>>>,
    last: &'a File<'a>,
}



#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_position() {
        let p = Position{filename: "test.gs", offset: 0, line: 5, column: 8};
        print!("this is the position: {} ", p)
    }
}

