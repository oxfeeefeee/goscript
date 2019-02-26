use std::fmt;
use std::fmt::Write;

pub struct Position {
    filename: &'static str,
    offset: usize,
    line: usize,
    column: usize,
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
    base: usize,
    size: usize,
    lines: Vec<usize>,
}

impl<'a> File<'a> {
    pub fn new(set: &'a FileSet<'a>, name: &'static str) -> File<'a> {
        File{set: set, name: name, base:0, size:0, lines: vec![]}
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn base(&self) -> usize {
        self.base
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn add_line(&mut self, offset: usize) {
        let i = self.line_count();
        if (i == 0 || self.lines[i-1] < offset) && offset < self.size {
            self.lines.push(offset);
        }
    }

    pub fn merge_line(&mut self, line: usize) {
        if line < 1 {
            panic!("illegal line number (line numbering starts at 1)")
        }
        if line >= self.line_count() {
            panic!("illegal line number")
        }
        /*
        let mut shalf = self.lines.split_off(line);
        self.lines.pop().unwrap();
        self.lines.append(&mut shalf);
        */
        let lines = &self.lines;
        self.lines = lines.into_iter().
            enumerate().
            filter(|&(i, _)| i != line).
            map(|(_, l)| *l).
            collect();
    }

    pub fn set_lines(&mut self, lines: Vec<usize>) -> bool {
        let size = self.size;
        for (i, &offset) in self.lines.iter().enumerate() {
            if (i == 0 &&size <= offset) || offset < lines[i-1] {
                return false;
            }
        }
        true
    }

}


pub struct FileSet<'a> {
    base: isize,
    files: Vec<Box<File<'a>>>,
    last: Option<&'a File<'a>>,
}

impl<'a> FileSet<'a> {
    pub fn new() -> FileSet<'a> {
        FileSet{base: 0, files: vec![], last: None}
    }
}


#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_position() {
        let p = Position{filename: "test.gs", offset: 0, line: 54321, column: 8};
        print!("this is the position: {} ", p);

        let fs = FileSet::new();
        let mut f = File::new(&fs, "test.gs");
        f.add_line(123);

    }
}

