use std::fmt;
use std::fmt::Write;
use std::rc::Rc;
use std::rc::Weak;
use std::cell::RefCell;

type Pos = usize;


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

#[derive(Debug)]
pub struct File {
    set: Weak<RefCell<FileSet>>,
    name: &'static str,
    base: usize,
    size: usize,
    lines: Vec<usize>,
}

impl File {
    pub fn new(set: Weak<RefCell<FileSet>>, name: &'static str) -> File {
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
        self.lines = lines;
        true
    }

    pub fn set_lines_for_content(&mut self, content: &[u8]) {
        /*
        let (mut new_line, mut line) = (false, 0);
        for (offset, b) in content.iter().enumerate() {
            if new_line {
                self.lines.push(line);
            }
            new_line = false;
            if *b == '\n' as u8 {
                new_line = true;
                line = offset + 1;
            }
        }*/
        self.lines = content.iter().
            enumerate().
            filter(|&(_, b)| *b == '\n' as u8).
            map(|(offset, _)| offset + 1).
            collect();
    }

    pub fn line_start(&self, line: usize) -> usize {
        if line < 1 {
            panic!("illegal line number (line numbering starts at 1)");
        }
        if line >= self.line_count() {
            panic!("illegal line number");
        }
        self.base + self.lines[line-1]
    }

    pub fn position(&self, p: Pos) -> Position {
        let filename = self.name;
        let (mut line, mut offset, mut column) = (0, 0, 0);
        if p > 0 {
            if p < self.base || p > self.base + self.size {
                panic!("illegal Pos value");
            }
            offset = p - self.base;
            let i = *(self.lines.iter().find(|&&x| x > offset).unwrap());
            line = i+1;
            column = offset - self.lines[i]+1;
        }
        Position{
            filename: filename,
            line: line,
            offset: offset,
            column: column,
        }
    } 
}

#[derive(Debug)]
pub struct FileSet {
    base: usize,
    files: Vec<Box<File>>,
}

impl FileSet {
    pub fn new() -> FileSet {
        FileSet{base: 0, files: vec![]}
    }

    pub fn base(&self) -> usize {
        self.base
    }
    
    pub fn add_file(set_ref: Rc<RefCell<FileSet>>, name: &'static str, base: isize, size: usize) {
        let mut set = set_ref.borrow_mut();
        let real_base = if base >=0 {base as usize} else {set.base};
        if real_base < set.base {
            panic!("illegal base");
        }

        let mut f = Box::new(File::new(Weak::new(), name));
        f.base = real_base;
        f.size = size;
        f.set = Rc::downgrade(&set_ref);
        
        let set_base = set.base + size + 1; // +1 because EOF also has a position
        if set_base < set.base {
            panic!("token.Pos offset overflow (> 2G of source code in file set)");
        }  
        set.base = set_base;
        set.files.push(f);
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_position() {
        let p = Position{filename: "test.gs", offset: 0, line: 54321, column: 8};
        print!("this is the position: {} ", p);
        /*
        let fs = FileSet::new();
        let mut f = Box::new(File::new(&fs, "test.gs"));
        f.size = 12345;
        f.add_line(123);
        f.add_line(133);
        f.add_line(143);
        print!("\nfile: {:?}", f);
        f.merge_line(1);
        print!("\nfile after merge: {:?}", f);
        f.set_lines_for_content(&['a' as u8 ,'\n' as u8, 'c' as u8]);
        print!("\nfile after set: {:?}", f);
        */
    }
}

