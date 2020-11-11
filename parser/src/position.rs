use std::borrow::Borrow;
use std::fmt;
use std::fmt::Write;
use std::rc::Rc;

pub type Pos = usize;

#[derive(Clone, Debug)]
pub struct Position {
    pub filename: Rc<String>,
    pub offset: usize, // offset in utf8 char
    pub line: usize,
    pub column: usize,
}

impl Position {
    pub fn is_valid(&self) -> bool {
        self.line > 0
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut s = String::clone(&*self.filename);
        if self.is_valid() {
            if s != "" {
                s.push(':');
            }
            s.push_str(&self.line.to_string());
        }
        if self.column != 0 {
            write!(&mut s, ":{}", self.column).unwrap();
        }
        if s.is_empty() {
            s.push('-');
        }
        f.write_str(&s)
    }
}

#[derive(Debug)]
pub struct File {
    name: Rc<String>,
    base: usize,
    size: usize,
    lines: Vec<usize>,
}

impl File {
    pub fn new(name: String) -> File {
        File {
            name: Rc::new(name),
            base: 0,
            size: 0,
            lines: vec![0],
        }
    }

    pub fn name(&self) -> &str {
        &self.name
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
        if (i == 0 || self.lines[i - 1] < offset) && offset < self.size {
            self.lines.push(offset);
        }
    }

    pub fn merge_line(&mut self, line: usize) {
        if line < 1 {
            panic!("illegal line number (line numbering starts at 1)");
        }
        if line >= self.line_count() {
            panic!("illegal line number");
        }
        /*
        let mut shalf = self.lines.split_off(line);
        self.lines.pop().unwrap();
        self.lines.append(&mut shalf);
        */
        let lines = &self.lines;
        self.lines = lines
            .into_iter()
            .enumerate()
            .filter(|&(i, _)| i != line)
            .map(|(_, l)| *l)
            .collect();
    }

    pub fn set_lines(&mut self, lines: Vec<usize>) -> bool {
        let size = self.size;
        for (i, &offset) in self.lines.iter().enumerate() {
            if (i == 0 && size <= offset) || offset < lines[i - 1] {
                return false;
            }
        }
        self.lines = lines;
        true
    }

    pub fn set_lines_for_content(&mut self, content: &mut std::str::Chars) {
        let (mut new_line, mut line) = (true, 0);
        for (offset, b) in content.enumerate() {
            if new_line {
                self.lines.push(line);
            }
            new_line = false;
            if b == '\n' {
                new_line = true;
                line = offset + 1;
            }
        }
    }

    pub fn line_start(&self, line: usize) -> usize {
        if line < 1 {
            panic!("illegal line number (line numbering starts at 1)");
        }
        if line >= self.line_count() {
            panic!("illegal line number");
        }
        self.base + self.lines[line - 1]
    }

    pub fn pos(&self, offset: usize) -> Pos {
        if offset > self.size() {
            panic!("illegal file offset")
        }
        self.base() + offset
    }

    pub fn position(&self, p: Pos) -> Position {
        if p < self.base || p > self.base + self.size {
            panic!("illegal Pos value");
        }

        let line_count = self.line_count();
        let offset = p - self.base;
        let line = match self
            .lines
            .iter()
            .enumerate()
            .find(|&(_, &line)| line > offset)
        {
            Some((i, _)) => i,
            None => line_count,
        };
        let column = offset - self.lines[line - 1] + 1;

        Position {
            filename: self.name.clone(),
            line: line,
            offset: offset,
            column: column,
        }
    }
}

#[derive(Debug)]
pub struct FileSet {
    base: usize,
    files: Vec<File>,
}

impl FileSet {
    pub fn new() -> FileSet {
        FileSet {
            base: 0,
            files: vec![],
        }
    }

    pub fn base(&self) -> usize {
        self.base
    }

    pub fn iter(&self) -> FileSetIter {
        FileSetIter { fs: self, cur: 0 }
    }

    pub fn file(&self, p: Pos) -> Option<&File> {
        for f in self.files.iter() {
            if f.base <= p && f.base + f.size >= p {
                return Some(f.borrow());
            }
        }
        None
    }

    pub fn position(&self, p: Pos) -> Position {
        if let Some(f) = self.file(p) {
            f.position(p)
        } else {
            return Position {
                filename: Rc::new("non_file_name".to_string()),
                line: 0,
                offset: 0,
                column: 0,
            };
        }
    }

    pub fn index_file(&mut self, i: usize) -> Option<&mut File> {
        if i >= self.files.len() {
            None
        } else {
            Some(&mut self.files[i])
        }
    }

    pub fn recent_file(&mut self) -> Option<&mut File> {
        let c = self.files.len();
        if c == 0 {
            None
        } else {
            self.index_file(c - 1)
        }
    }

    pub fn add_file(&mut self, name: String, base: Option<usize>, size: usize) -> &mut File {
        let real_base = if let Some(b) = base { b } else { self.base };
        if real_base < self.base {
            panic!("illegal base");
        }

        let mut f = File::new(name);
        f.base = real_base;
        f.size = size;
        let set_base = self.base + size + 1; // +1 because EOF also has a position
        if set_base < self.base {
            panic!("token.Pos offset overflow (> 2G of source code in file set)");
        }
        self.base = set_base;
        self.files.push(f);
        self.recent_file().unwrap()
    }
}

pub struct FileSetIter<'a> {
    fs: &'a FileSet,
    cur: usize,
}

impl<'a> Iterator for FileSetIter<'a> {
    type Item = &'a File;

    fn next(&mut self) -> Option<&'a File> {
        if self.cur < self.fs.files.len() {
            self.cur += 1;
            Some(self.fs.files[self.cur - 1].borrow())
        } else {
            None
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_position() {
        let p = Position {
            filename: Rc::new("test.gs".to_string()),
            offset: 0,
            line: 54321,
            column: 8,
        };
        print!("this is the position: {} ", p);
        let mut fs = FileSet::new();
        let mut f = File::new("test.gs".to_string());
        f.size = 12345;
        f.add_line(123);
        f.add_line(133);
        f.add_line(143);
        print!("\nfile: {:?}", f);
        f.merge_line(1);
        print!("\nfile after merge: {:?}", f);

        {
            fs.add_file("testfile1.gs".to_string(), None, 222);
            fs.add_file("testfile2.gs".to_string(), None, 222);
            fs.add_file("testfile3.gs".to_string(), None, 222);
            print!("\nset {:?}", fs);
        }

        for f in fs.iter() {
            print!("\nfiles in set: {:?}", f);
        }
        print!("\nfile at 100: {:?}", fs.file(100))
    }
}
