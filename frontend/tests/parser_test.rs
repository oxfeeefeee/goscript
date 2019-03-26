extern crate goscript_frontend as fe;
use std::fs;


fn load_parse(path: &'static str) {
    let mut fs = fe::FileSet::new();
    let src = fs::read_to_string(path).expect("read file err: ");
    let p = fe::parse_file(&mut fs, path, &src);
    print!("{}", p.get_errors());

}

#[test] 
fn test_parser1 () {
    load_parse("./tests/data/case1.gos");

    /*let mut fs = fe::FileSet::new();
    let f = fs.add_file("testfile1.gs", None, 1000);

    let s1 = r###"
    func (p *someobj111) testFunc(a, b *int) (i int) {
        for i := range iii {
            a = 1;
        }
    } 
    "###;
    
    let mut p = fe::Parser::new(f, s1, true);
    p.open_scope();
    p.pkg_scope = p.top_scope;
    p.next();
    p.parse_decl(fe::Token::is_decl_start);
    */
}