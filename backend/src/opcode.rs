/*
    Based on lua opcode
  +---------------------------------------------+
  |0-5(6bits)|6-13(8bit)|14-22(9bit)|23-31(9bit)|
  |==========+==========+===========+===========|
  |  opcode  |    A     |     C     |    B      |
  |----------+----------+-----------+-----------|
  |  opcode  |    A     |      Bx(unsigned)     |
  |----------+----------+-----------+-----------|
  |  opcode  |    A     |      sBx(signed)      |
  +---------------------------------------------+
*/

struct Opcode(u32);

impl Opcode {
    #[inline]
    pub fn a(&self) -> u32 {
        0
    }
}

#[cfg(test)]
mod test {
    #[test]
	fn test_opcode() {
        println!("opcode\n"); 
    }
}