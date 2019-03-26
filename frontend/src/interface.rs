use super::parser;
use super::position;

pub fn parse_file<'a>(fs: &'a mut position::FileSet, name: &'static str, src: &'a str) -> parser::Parser<'a> {
    let f = fs.add_file(name, None, src.chars().count());
    let p = parser::Parser::new(f, src, true);
    p
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn test_stuff () {
        let mut fs = position::FileSet::new();
        parse_file(&mut fs, "test.gs", "1+2");
    }
}
